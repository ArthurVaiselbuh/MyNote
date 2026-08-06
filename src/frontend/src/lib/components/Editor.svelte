<script lang="ts">
  import { onMount } from "svelte";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
  import { syntaxHighlighting } from "@codemirror/language";
  import { languages } from "@codemirror/language-data";
  import {
    closeSearchPanel,
    findNext as cmFindNext,
    findPrevious as cmFindPrev,
    openSearchPanel,
    search,
    searchKeymap,
    searchPanelOpen,
    SearchQuery,
    setSearchQuery,
  } from "@codemirror/search";
  import { EditorState } from "@codemirror/state";
  import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
  import { drawSelection, EditorView, keymap } from "@codemirror/view";
  import * as act from "../actions";
  import { api } from "../api";
  import { editorCtl } from "../editorCtl";
  import { hintOf, labelOf } from "../keys/bindings";
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
  let lastEditorReq = 0;
  let lastTitleReq = 0;

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

  const extensions = [
    history(),
    drawSelection(),
    EditorView.lineWrapping,
    markdown({ base: markdownLanguage, codeLanguages: languages }),
    syntaxHighlighting(oneDarkHighlightStyle, { fallback: true }),
    search({ top: true }),
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
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

  const AUTOSAVE_MS = 3000;

  function scheduleSave() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), AUTOSAVE_MS);
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

  function heightAtViewportTop(): number {
    return view!.scrollDOM.getBoundingClientRect().top - view!.documentTop;
  }

  function editorAnchor(): ModeAnchor | null {
    if (!view || !loadedId) return null;
    const heightAtTop = heightAtViewportTop();
    const block = view.lineBlockAtHeight(Math.max(0, heightAtTop));
    return {
      pageId: loadedId,
      bodyLine: view.state.doc.lineAt(block.from).number - 1,
      offsetFromTop: block.top - heightAtTop,
    };
  }

  function rememberEditorPos() {
    if (!view || !loadedId) return;
    const pos = viewPosOf(loadedId);
    pos.editorCursor = view.state.selection.main.head;
    // in preview the host is display:none, so its scrollTop is not the one the
    // user left behind — that was recorded when the mode flipped
    if (app.mode === "edit") pos.editorScrollTop = view.scrollDOM.scrollTop;
    viewPosChanged(loadedId);
  }

  function cursorAtAnchor(anchor: ModeAnchor): number {
    const doc = view!.state.doc;
    const line = doc.line(Math.min(anchor.bodyLine + 1, doc.lines));
    const column = anchor.text ? line.text.toLowerCase().indexOf(anchor.text.toLowerCase()) : -1;
    return column < 0 ? line.from : line.from + column;
  }

  function restoreEditorPos(pageId: string) {
    if (!view) return;
    const pos = viewPosOf(pageId);
    const cursor = Math.min(pos.editorCursor, view.state.doc.length);
    const scrollTop = pos.editorScrollTop;
    view.dispatch({ selection: { anchor: cursor } });
    requestAnimationFrame(() => {
      if (view) view.scrollDOM.scrollTop = scrollTop;
    });
  }

  // only a deliberate mode flip carries an anchor; entering the editor any other
  // way (insert helper, revision restore) keeps the position that path chose
  function landOnModeAnchor(pageId: string) {
    const anchor = takeModeAnchor(pageId);
    if (!view || !anchor) return;
    const cursor = cursorAtAnchor(anchor);
    viewPosOf(pageId).editorCursor = cursor;
    viewPosChanged(pageId);
    view.dispatch({ selection: { anchor: cursor } });
    requestAnimationFrame(() => {
      if (!view) return;
      view.scrollDOM.scrollTop +=
        view.lineBlockAt(cursor).top - anchor.offsetFromTop - heightAtViewportTop();
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
      if (app.mode === "edit") restoreEditorPos(id);
      if (app.findPrefill) {
        const prefill = app.findPrefill;
        app.findPrefill = null;
        // opening a search result lands in preview, so the prefill has to reach
        // the preview's find rather than force the page back into the editor
        requestAnimationFrame(() => act.activeFindCtl()?.openFind(prefill));
      }
    } catch (e) {
      app.status = String(e);
    }
  }

  function ctlOpenFind(prefill?: FindPrefill | null) {
    if (!view) return;
    if (app.mode !== "edit") app.mode = "edit";
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

  async function insertImage(file: File, ext: string) {
    if (!view || !loadedId) return;
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      const chunk = 0x8000;
      for (let i = 0; i < bytes.length; i += chunk) {
        binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
      }
      const rel = await api.saveImage(loadedId, btoa(binary), ext);
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
    void switchTo(app.currentPageId);
  });

  $effect(() => {
    if (app.editorFocusReq !== lastEditorReq) {
      lastEditorReq = app.editorFocusReq;
      // gate on app.focus: a remount (leaving results view) replays the last
      // request, which must not steal focus aimed at another pane — nor pull
      // keystrokes under an open modal (Ctrl+H from results remounts us)
      if (app.mode === "edit" && app.focus === "editor" && app.modal === "none") view?.focus();
    }
  });

  $effect(() => {
    if (app.titleFocusReq !== lastTitleReq) {
      lastTitleReq = app.titleFocusReq;
      if (app.focus !== "editor") return;
      requestAnimationFrame(() => {
        titleInput?.focus();
        titleInput?.select();
      });
    }
  });

  let shownMode = app.mode;
  $effect(() => {
    const mode = app.mode;
    if (!view || mode === shownMode) return;
    shownMode = mode;
    if (mode === "preview") {
      previewText = view.state.doc.toString();
      void save();
    } else if (loadedId) {
      landOnModeAnchor(loadedId);
    }
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
      closeFind() {
        if (view && searchPanelOpen(view.state)) {
          closeSearchPanel(view);
          return true;
        }
        return false;
      },
      findOpen: () => !!view && searchPanelOpen(view.state),
      findNext() {
        if (view) cmFindNext(view);
      },
      findPrev() {
        if (view) cmFindPrev(view);
      },
      insert(before: string, after = "") {
        if (!view) return;
        if (app.mode !== "edit") app.mode = "edit";
        const { from, to } = view.state.selection.main;
        const selected = view.state.sliceDoc(from, to);
        const text = before + selected + after;
        const anchor = selected ? from + text.length : from + before.length;
        view.dispatch({ changes: { from, to, insert: text }, selection: { anchor } });
        view.focus();
      },
      setTitle(t: string) {
        title = t;
      },
      replaceAll(content: string) {
        if (!view || !loadedId) return;
        if (app.mode !== "edit") app.mode = "edit";
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
      },
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
