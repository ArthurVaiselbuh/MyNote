<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import * as act from "./lib/actions";
  import { handleGlobal } from "./lib/keys/dispatch";
  import { app } from "./lib/state/app.svelte";
  import { isTextEntry } from "./lib/textEntry";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import Editor from "./lib/components/Editor.svelte";
  import ResultPeek from "./lib/components/ResultPeek.svelte";
  import Results from "./lib/components/Results.svelte";
  import SearchBar from "./lib/components/SearchBar.svelte";
  import SectionStrip from "./lib/components/SectionStrip.svelte";
  import Tree from "./lib/components/Tree.svelte";
  import ConfirmDialog from "./lib/components/modals/ConfirmDialog.svelte";
  import HelpOverlay from "./lib/components/modals/HelpOverlay.svelte";
  import History from "./lib/components/modals/History.svelte";
  import Import from "./lib/components/modals/Import.svelte";
  import ColorPicker from "./lib/components/modals/ColorPicker.svelte";
  import InsertHelper from "./lib/components/modals/InsertHelper.svelte";
  import OpenNotebook from "./lib/components/modals/OpenNotebook.svelte";
  import SectionPicker from "./lib/components/modals/SectionPicker.svelte";
  import SettingsModal from "./lib/components/modals/SettingsModal.svelte";
  import ColorsModal from "./lib/components/modals/ColorsModal.svelte";
  import KeybindingsModal from "./lib/components/modals/KeybindingsModal.svelte";
  import Welcome from "./lib/components/Welcome.svelte";

  const s = $derived(app.settings);

  // the history pane stays mounted behind the modals it opens itself, so backing
  // out of them lands on an intact pane instead of a flash of the editor
  const historyOpen = $derived(
    app.modal === "history" ||
      (app.modal === "help" && app.helpContext === "history") ||
      app.confirm?.returnTo === "history",
  );

  let draggingSplitter = $state<"tree" | "peek" | null>(null);

  // the tree grows rightward from the left edge, the peek leftward from the
  // right one, so each maps the pointer to its own width
  function dragSplitter(e: PointerEvent, which: "tree" | "peek") {
    const splitter = e.currentTarget as HTMLElement;
    const widthAt = (x: number) => (which === "tree" ? x : window.innerWidth - x);
    const setWidth = which === "tree" ? act.setTreeWidth : act.setPeekWidth;
    const grabbed = app.settings[which === "tree" ? "treeWidth" : "peekWidth"] - widthAt(e.clientX);
    splitter.setPointerCapture(e.pointerId);
    draggingSplitter = which;
    const move = (ev: PointerEvent) => setWidth(widthAt(ev.clientX) + grabbed);
    const stop = () => {
      splitter.removeEventListener("pointermove", move);
      splitter.removeEventListener("pointerup", stop);
      splitter.removeEventListener("pointercancel", stop);
      draggingSplitter = null;
      void act.persistSettings();
    };
    splitter.addEventListener("pointermove", move);
    splitter.addEventListener("pointerup", stop);
    splitter.addEventListener("pointercancel", stop);
  }

  function resetTreeWidth() {
    act.setTreeWidth(act.TREE_WIDTH_DEFAULT);
    void act.persistSettings();
  }

  function resetPeekWidth() {
    act.setPeekWidth(act.PEEK_WIDTH_DEFAULT);
    void act.persistSettings();
  }

  onMount(() => {
    window.addEventListener("keydown", handleGlobal, true);

    const wheel = (e: WheelEvent) => {
      const speed = app.settings.scrollSpeed;
      if (speed === 1 || e.ctrlKey) return;
      const scroller = (e.target as HTMLElement | null)?.closest(
        ".cm-scroller, .preview, .tree, .results",
      );
      if (scroller instanceof HTMLElement) {
        e.preventDefault();
        scroller.scrollBy({ top: e.deltaY * speed });
      }
    };
    window.addEventListener("wheel", wheel, { passive: false });

    // the webview's own menu (refresh / print / save as / send tab to your
    // devices) is meaningless here, so panes raise their own via openContextMenu
    // — but text fields keep the native one, the only place right-click paste
    // and spell-check suggestions come from
    const suppressWebviewMenu = (e: MouseEvent) => {
      if (isTextEntry(e.target)) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", suppressWebviewMenu);

    const flushSave = () => void act.saveNow();
    window.addEventListener("blur", flushSave);

    let unlistenFlushAndClose: (() => void) | undefined;
    void listen("mynote:flush-and-close", async () => {
      await act.saveNow();
      await getCurrentWindow().close();
    }).then((un) => {
      unlistenFlushAndClose = un;
    });

    let unlistenFlush: (() => void) | undefined;
    void listen("mynote:flush", flushSave).then((un) => {
      unlistenFlush = un;
    });

    let unlistenGitSnapshotFailed: (() => void) | undefined;
    void listen<string>("mynote:git-snapshot-failed", (event) => {
      act.flashStatusError(event.payload);
    }).then((un) => {
      unlistenGitSnapshotFailed = un;
    });

    void act.boot();

    return () => {
      window.removeEventListener("keydown", handleGlobal, true);
      window.removeEventListener("wheel", wheel);
      window.removeEventListener("contextmenu", suppressWebviewMenu);
      window.removeEventListener("blur", flushSave);
      unlistenFlushAndClose?.();
      unlistenFlush?.();
      unlistenGitSnapshotFailed?.();
    };
  });
</script>

<div
  class="app"
  style="--text:{s.textColor}; --bg:{s.backgroundColor}; --panel:{s.panelColor}; --accent:{s.accentColor}; --heading:{s.headingColor}; --focus-alpha:{s.focusAlpha}"
>
  <aside
    class="tree-pane"
    class:focused={app.focus === "tree"}
    style:width="{s.treeWidth}px"
  >
    <SectionStrip />
    <Tree />
  </aside>
  <div
    class="splitter"
    class:dragging={draggingSplitter === "tree"}
    role="separator"
    aria-orientation="vertical"
    onpointerdown={(e) => dragSplitter(e, "tree")}
    ondblclick={resetTreeWidth}
  ></div>
  <main class="main-pane">
    {#if app.view === "results"}
      <SearchBar />
      <div class="results-split">
        <Results />
        <div
          class="splitter"
          class:dragging={draggingSplitter === "peek"}
          role="separator"
          aria-orientation="vertical"
          onpointerdown={(e) => dragSplitter(e, "peek")}
          ondblclick={resetPeekWidth}
        ></div>
        <ResultPeek />
      </div>
    {:else}
      <Editor />
    {/if}
  </main>

  {#if app.modal === "help"}<HelpOverlay />{/if}
  {#if app.modal === "settings"}<SettingsModal />{/if}
  {#if app.modal === "colors"}<ColorsModal />{/if}
  {#if app.modal === "keybindings"}<KeybindingsModal />{/if}
  {#if app.modal === "sectionPicker"}<SectionPicker />{/if}
  {#if app.modal === "confirm"}<ConfirmDialog />{/if}
  {#if app.modal === "insert"}<InsertHelper />{/if}
  {#if app.modal === "colorPicker"}<ColorPicker />{/if}
  {#if app.modal === "import"}<Import />{/if}
  {#if app.modal === "openNotebook"}<OpenNotebook />{/if}
  <!-- full-window: covers tree + main, but leaves the Editor mounted so restore can
       reach editorCtl -->
  {#if historyOpen}<History />{/if}

  <!-- always mounted: owns its own first-run trigger and visibility -->
  <Welcome />
  <ContextMenu />

  {#if app.status}
    <div class="status-toast" class:error={app.statusIsError}>{app.status}</div>
  {/if}
</div>
