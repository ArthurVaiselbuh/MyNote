<script lang="ts" module>
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
  import { syntaxHighlighting } from "@codemirror/language";
  import { languages } from "@codemirror/language-data";
  import { search, searchKeymap } from "@codemirror/search";
  import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
  import { drawSelection, EditorView, keymap } from "@codemirror/view";

  const cmTheme = EditorView.theme(
    {
      "&": { backgroundColor: "transparent", color: "var(--text)" },
      ".cm-content": {
        caretColor: "var(--accent)",
        fontFamily: "Consolas, monospace",
        fontSize: "14px",
        paddingBottom: "40vh",
      },
      ".cm-cursor": { borderLeftColor: "var(--accent)" },
      "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
        backgroundColor: "color-mix(in srgb, var(--accent) 30%, transparent)",
      },
      ".cm-activeLine": { backgroundColor: "transparent" },
      ".cm-panels": {
        backgroundColor: "var(--panel)",
        color: "var(--text)",
        borderColor: "var(--guide)",
      },
      ".cm-panels input": { background: "var(--bg)" },
      ".cm-panels button": { color: "var(--text)" },
      ".cm-searchMatch": {
        backgroundColor: "color-mix(in srgb, var(--accent) 30%, transparent)",
      },
      ".cm-searchMatch-selected": {
        backgroundColor: "color-mix(in srgb, var(--accent) 55%, transparent)",
      },
    },
    { dark: true },
  );

  // Everything that doesn't close over an instance is built once: this component
  // remounts every time the results view is left, and a fresh theme means
  // another copy of its stylesheet injected into the document for the session.
  const BASE_EXTENSIONS = [
    history(),
    drawSelection(),
    EditorView.lineWrapping,
    markdown({ base: markdownLanguage, codeLanguages: languages }),
    syntaxHighlighting(oneDarkHighlightStyle, { fallback: true }),
    search({ top: true }),
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
  ];

  const AUTOSAVE_MS = 3000;
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    closeSearchPanel,
    findNext as cmFindNext,
    findPrevious as cmFindPrev,
    openSearchPanel,
    searchPanelOpen,
    SearchQuery,
    setSearchQuery,
  } from "@codemirror/search";
  import { EditorState } from "@codemirror/state";
  import * as act from "../actions";
  import { api } from "../api";
  import { focusSelect } from "../autofocus";
  import { hintOf, labelOf } from "../keys/bindings";
  import { blockRangeAt } from "../markdown";
  import { onRequest } from "../onRequest.svelte";
  import { editorCtl } from "../paneCtl";
  import { app, type FindPrefill } from "../state/app.svelte";
  import { findNode } from "../treeUtils";
  import { takeModeAnchor, viewPosChanged, viewPosOf, type ModeAnchor } from "../viewPos";
  import Preview from "./Preview.svelte";

  let host: HTMLElement | undefined = $state();
  let titleInput: HTMLInputElement | undefined = $state();
  let title = $state("");
  let previewText = $state("");
  let dirty = $state(false);

  let view: EditorView | undefined;
  let loadedId: string | null = null;
  let loadSeq = 0;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  const extensions = [
    ...BASE_EXTENSIONS,
    EditorView.updateListener.of((u) => {
      if (u.docChanged) {
        dirty = true;
        scheduleSave();
      }
    }),
    EditorView.domEventHandlers({
      paste: handlePaste,
      focus: () => {
        app.focus = "editor";
      },
    }),
    cmTheme,
  ];

  function scheduleSave() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), AUTOSAVE_MS);
  }

  function enterEditMode() {
    if (app.mode !== "edit") app.mode = "edit";
  }

  // the view can be destroyed between a dispatch and the frame that measures it
  function afterLayout(run: (v: EditorView) => void) {
    requestAnimationFrame(() => {
      if (view) run(view);
    });
  }

  function splitDoc(content: string): { title: string; body: string } {
    const lines = content.split("\n");
    let i = 0;
    while (i < lines.length && lines[i].trim() === "") i++;
    if (i < lines.length && lines[i].trim().startsWith("# ")) {
      const t = lines[i].trim().slice(2).trim();
      let j = i + 1;
      if (j < lines.length && lines[j].trim() === "") j++;
      return { title: t, body: lines.slice(j).join("\n") };
    }
    return { title: "", body: content };
  }

  async function save() {
    clearTimeout(saveTimer);
    rememberEditorPos();
    if (!view || !loadedId || !dirty) return;
    dirty = false;
    const id = loadedId;
    const content = `# ${title.trim() || "Untitled"}\n\n${view.state.doc.toString()}`;
    try {
      const newTitle = await api.writePage(id, content);
      act.updateTreeTitle(id, newTitle);
      if (title.trim() !== "" && newTitle !== title) title = newTitle;
    } catch (e) {
      dirty = true;
      app.status = String(e);
    }
  }

  function setDoc(body: string) {
    view?.setState(EditorState.create({ doc: body, extensions }));
  }

  function heightAtViewportTop(v: EditorView): number {
    return v.scrollDOM.getBoundingClientRect().top - v.documentTop;
  }

  function editorAnchor(): ModeAnchor | null {
    if (!view || !loadedId) return null;
    const heightAtTop = heightAtViewportTop(view);
    const block = view.lineBlockAtHeight(Math.max(0, heightAtTop));
    const bodyLine = view.state.doc.lineAt(block.from).number - 1;
    const dims = blockHeight(view, blockRangeAt(view.state.doc.toString(), bodyLine));
    return {
      pageId: loadedId,
      bodyLine,
      offsetRatio: dims.height > 0 ? clampRatio((heightAtTop - dims.top) / dims.height) : 0,
    };
  }

  function clampRatio(x: number): number {
    return Math.min(1, Math.max(0, x));
  }

  function lineTop(v: EditorView, bodyLine: number): number {
    return v.lineBlockAt(v.state.doc.line(Math.min(bodyLine + 1, v.state.doc.lines)).from).top;
  }

  // a preview block (paragraph, list item, fenced code block) can merge many
  // source lines into one rendered element — this is the editor's own height
  // across that same body-line span, so offsetRatio means the same fraction on
  // both sides of the toggle instead of "fraction of one CM line" vs "fraction
  // of the whole merged block"
  function blockHeight(v: EditorView, range: { start: number; end: number }): { top: number; height: number } {
    const top = lineTop(v, range.start);
    const totalLines = v.state.doc.lines;
    const bottom =
      range.end >= totalLines ? v.lineBlockAt(v.state.doc.line(totalLines).from).bottom : lineTop(v, range.end);
    return { top, height: bottom - top };
  }

  function rememberEditorPos() {
    if (!view || !loadedId) return;
    const pos = viewPosOf(loadedId);
    const cursor = view.state.selection.main.head;
    // in preview the host is display:none, so its scrollTop is not the one the
    // user left behind — that was recorded when the mode flipped
    const scrollTop = app.mode === "edit" ? view.scrollDOM.scrollTop : pos.editorScrollTop;
    if (pos.editorCursor === cursor && pos.editorScrollTop === scrollTop) return;
    pos.editorCursor = cursor;
    pos.editorScrollTop = scrollTop;
    viewPosChanged(loadedId);
  }

  function lineAtAnchor(v: EditorView, anchor: ModeAnchor) {
    return v.state.doc.line(Math.min(anchor.bodyLine + 1, v.state.doc.lines));
  }

  function cursorAtAnchor(v: EditorView, anchor: ModeAnchor): number {
    const line = lineAtAnchor(v, anchor);
    const column = anchor.text ? line.text.toLowerCase().indexOf(anchor.text.toLowerCase()) : -1;
    return column < 0 ? line.from : line.from + column;
  }

  function restoreEditorPos() {
    if (!view || !loadedId) return;
    const pos = viewPosOf(loadedId);
    const scrollTop = pos.editorScrollTop;
    view.dispatch({ selection: { anchor: Math.min(pos.editorCursor, view.state.doc.length) } });
    afterLayout((v) => (v.scrollDOM.scrollTop = scrollTop));
  }

  // only a deliberate mode flip carries an anchor; entering the editor any other
  // way (insert helper, revision restore) keeps the position that path chose.
  // The anchor's line is the top-of-viewport target for scroll — the caret
  // itself only follows it when landing on a preview find match; otherwise the
  // real caret spot (saved by rememberEditorPos before preview took over) wins.
  function landOnModeAnchor() {
    if (!view || !loadedId) return;
    const anchor = takeModeAnchor(loadedId);
    if (!anchor) return;
    const pos = viewPosOf(loadedId);
    const cursor = anchor.text
      ? cursorAtAnchor(view, anchor)
      : Math.min(pos.editorCursor, view.state.doc.length);
    pos.editorCursor = cursor;
    viewPosChanged(loadedId);
    view.dispatch({ selection: { anchor: cursor } });
    afterLayout((v) => {
      const dims = blockHeight(v, blockRangeAt(v.state.doc.toString(), anchor.bodyLine));
      v.scrollDOM.scrollTop += dims.top + anchor.offsetRatio * dims.height - heightAtViewportTop(v);
    });
  }

  async function switchTo(id: string | null) {
    if (id === loadedId) return;
    const seq = ++loadSeq;
    await save();
    if (seq !== loadSeq) return;
    loadedId = id;
    if (!id) {
      title = "";
      setDoc("");
      previewText = "";
      dirty = false;
      return;
    }
    try {
      const content = await api.readPage(id);
      if (seq !== loadSeq) return;
      const parsed = splitDoc(content);
      const section = act.currentSection();
      title = parsed.title || (section && findNode(section.pages, id)?.title) || "Untitled";
      setDoc(parsed.body);
      previewText = parsed.body;
      dirty = false;
      // in preview the editor is hidden and unmeasurable — its position is
      // restored when the mode flips back
      if (app.mode === "edit") restoreEditorPos();
      if (app.findPrefill) {
        const prefill = app.findPrefill;
        app.findPrefill = null;
        // opening a search result lands in preview, so the prefill has to reach
        // the preview's find rather than force the page back into the editor
        requestAnimationFrame(() => act.activePaneCtl()?.openFind(prefill));
      }
    } catch (e) {
      app.status = String(e);
    }
  }

  function findIsOpen(): boolean {
    return !!view && searchPanelOpen(view.state);
  }

  function ctlOpenFind(prefill?: FindPrefill | null) {
    if (!view) return;
    enterEditMode();
    openSearchPanel(view);
    if (prefill?.text) {
      view.dispatch({
        effects: setSearchQuery.of(
          new SearchQuery({ search: prefill.text, caseSensitive: false, regexp: prefill.regex }),
        ),
      });
      cmFindNext(view);
    }
  }

  function ctlCloseFind(): boolean {
    if (!view || !findIsOpen()) return false;
    closeSearchPanel(view);
    return true;
  }

  function ctlFindNext() {
    if (view) cmFindNext(view);
  }

  function ctlFindPrev() {
    if (view) cmFindPrev(view);
  }

  function ctlInsert(before: string, after = "") {
    if (!view) return;
    enterEditMode();
    const { from, to } = view.state.selection.main;
    const selected = view.state.sliceDoc(from, to);
    const text = before + selected + after;
    const anchor = selected ? from + text.length : from + before.length;
    view.dispatch({ changes: { from, to, insert: text }, selection: { anchor } });
    view.focus();
  }

  function ctlReplaceAll(content: string) {
    if (!view || !loadedId) return;
    enterEditMode();
    const parsed = splitDoc(content);
    if (parsed.title) {
      title = parsed.title;
      act.updateTreeTitle(loadedId, parsed.title);
    }
    // a real transaction over the whole doc — NOT setState(), which would
    // wipe CodeMirror's undo history and make this restore un-undoable
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: parsed.body },
      selection: { anchor: 0 },
      scrollIntoView: true,
    });
    view.focus();
  }

  function handlePaste(e: ClipboardEvent): boolean {
    const items = e.clipboardData?.items;
    if (!items || !loadedId) return false;
    for (const item of items) {
      if (item.type.startsWith("image/")) {
        const file = item.getAsFile();
        if (!file) continue;
        e.preventDefault();
        const ext = (item.type.split("/")[1] ?? "png").replace("jpeg", "jpg");
        void insertImage(file, ext);
        return true;
      }
    }
    return false;
  }

  // save_image takes base64 over IPC; readAsDataURL produces it natively, where
  // btoa would first need the whole image copied into a binary string
  function base64Of(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(reader.error);
      reader.onload = () => {
        const dataUrl = reader.result as string;
        resolve(dataUrl.slice(dataUrl.indexOf(",") + 1));
      };
      reader.readAsDataURL(blob);
    });
  }

  async function insertImage(file: File, ext: string) {
    if (!view || !loadedId) return;
    try {
      const rel = await api.saveImage(loadedId, await base64Of(file), ext);
      const pos = view.state.selection.main.head;
      view.dispatch({
        changes: { from: pos, insert: `![](${rel})` },
        selection: { anchor: pos + 2 },
      });
    } catch (e) {
      app.status = String(e);
    }
  }

  $effect(() => {
    const id = app.currentPageId;
    untrack(() => void switchTo(id));
  });

  onRequest(
    () => app.editorFocusReq,
    () => {
      // gate on app.focus: a remount (leaving results view) replays the last
      // request, which must not steal focus aimed at another pane — nor pull
      // keystrokes under an open modal (Ctrl+H from results remounts us)
      if (app.mode === "edit" && app.focus === "editor" && app.modal === "none") view?.focus();
    },
  );

  onRequest(
    () => app.titleFocusReq,
    () => {
      if (app.focus !== "editor") return;
      requestAnimationFrame(() => focusSelect(titleInput));
    },
  );

  let shownMode = app.mode;
  $effect(() => {
    const mode = app.mode;
    const v = view;
    if (!v || mode === shownMode) return;
    shownMode = mode;
    untrack(() => {
      if (mode === "preview") {
        previewText = v.state.doc.toString();
        void save();
      } else {
        landOnModeAnchor();
      }
    });
  });

  function titleKeys(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === "ArrowDown") {
      e.preventDefault();
      if (app.mode === "edit") view?.focus();
    }
  }

  onMount(() => {
    view = new EditorView({
      parent: host!,
      state: EditorState.create({ doc: "", extensions }),
    });
    editorCtl.current = {
      save,
      anchor: editorAnchor,
      openFind: ctlOpenFind,
      closeFind: ctlCloseFind,
      findOpen: findIsOpen,
      findNext: ctlFindNext,
      findPrev: ctlFindPrev,
      scroller: () => view?.scrollDOM ?? null,
      insert: ctlInsert,
      replaceAll: ctlReplaceAll,
      setTitle: (t: string) => (title = t),
    };
    return () => {
      void save();
      editorCtl.current = null;
      view?.destroy();
      view = undefined;
    };
  });
</script>

<div class="editor-wrap pane-focusable" class:focused={app.focus === "editor"}>
  {#if app.currentPageId}
    <div class="editor-head">
      <input
        class="title-input"
        placeholder="Untitled"
        bind:this={titleInput}
        bind:value={title}
        oninput={() => {
          dirty = true;
          scheduleSave();
        }}
        onkeydown={titleKeys}
        onfocus={() => (app.focus = "editor")}
      />
      <span class="dirty-dot">{dirty ? "●" : ""}</span>
      <button class="mode-btn" title="Toggle edit/preview{hintOf('app.toggleMode')}" onclick={() => act.toggleMode()}>
        {app.mode === "edit" ? "Preview" : "Edit"}
      </button>
    </div>
  {/if}
  <div
    class="cm-host"
    bind:this={host}
    style:display={app.currentPageId && app.mode === "edit" ? "" : "none"}
  ></div>
  {#if app.currentPageId && app.mode === "preview"}
    <Preview body={previewText} />
  {/if}
  {#if !app.currentPageId}
    <div class="editor-empty">No page selected — {labelOf('page.new')} creates one</div>
  {/if}
</div>
