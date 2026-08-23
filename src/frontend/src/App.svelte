<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import * as act from "./lib/actions";
  import type { SplitPane } from "./lib/actions";
  import { contextMenu } from "./lib/contextMenu.svelte";
  import { handleGlobal } from "./lib/keys/dispatch";
  import { app, modalParentOf } from "./lib/state/app.svelte";
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
  const historyOpen = $derived(app.modal === "history" || modalParentOf() === "history");

  let draggingSplitter = $state<SplitPane | null>(null);

  // the tree grows rightward from the left edge, the peek leftward from the
  // right one, so each maps the pointer to its own width
  const WIDTH_AT: Record<SplitPane, (x: number) => number> = {
    tree: (x) => x,
    peek: (x) => window.innerWidth - x,
  };

  function dragSplitter(e: PointerEvent, pane: SplitPane) {
    const splitter = e.currentTarget as HTMLElement;
    const widthAt = WIDTH_AT[pane];
    const grabbed = act.paneWidth(pane) - widthAt(e.clientX);
    const drag = new AbortController();
    const stop = () => {
      drag.abort();
      draggingSplitter = null;
      void act.persistSettings();
    };
    splitter.setPointerCapture(e.pointerId);
    draggingSplitter = pane;
    splitter.addEventListener(
      "pointermove",
      (ev: PointerEvent) => act.setPaneWidth(pane, widthAt(ev.clientX) + grabbed),
      { signal: drag.signal },
    );
    splitter.addEventListener("pointerup", stop, { signal: drag.signal });
    splitter.addEventListener("pointercancel", stop, { signal: drag.signal });
  }

  function resetWidth(pane: SplitPane) {
    act.resetPaneWidth(pane);
    void act.persistSettings();
  }

  function navigateViewedPages(e: MouseEvent) {
    const direction = e.button === 3 ? -1 : e.button === 4 ? 1 : null;
    if (!direction) return;
    e.preventDefault();
    if (app.modal !== "none" || contextMenu.open) return;
    act.navigateViewedPages(direction);
  }

  // registered only while it will act: a non-passive wheel listener costs the
  // compositor its scroll fast path even when the handler returns immediately
  $effect(() => {
    const speed = app.settings.scrollSpeed;
    if (speed === 1) return;
    const wheel = (e: WheelEvent) => {
      if (e.ctrlKey) return;
      const scroller = (e.target as HTMLElement | null)?.closest(
        ".cm-scroller, .preview, .tree, .results",
      );
      if (scroller instanceof HTMLElement) {
        e.preventDefault();
        scroller.scrollBy({ top: e.deltaY * speed });
      }
    };
    window.addEventListener("wheel", wheel, { passive: false });
    return () => window.removeEventListener("wheel", wheel);
  });

  onMount(() => {
    window.addEventListener("keydown", handleGlobal, true);
    window.addEventListener("mousedown", navigateViewedPages, true);

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

    const unlisteners: (() => void)[] = [];
    const subscribe = (pending: Promise<() => void>) =>
      void pending.then((un) => unlisteners.push(un));
    const flashPayload = (event: { payload: string }) => act.flashStatusError(event.payload);

    subscribe(
      listen("mynote:flush-and-close", async () => {
        await act.saveNow();
        await getCurrentWindow().close();
      }),
    );
    subscribe(listen("mynote:flush", flushSave));
    subscribe(listen<string>("mynote:git-snapshot-failed", flashPayload));
    subscribe(listen<string>("mynote:hotkey-unavailable", flashPayload));

    void act.boot();

    return () => {
      window.removeEventListener("keydown", handleGlobal, true);
      window.removeEventListener("mousedown", navigateViewedPages, true);
      window.removeEventListener("contextmenu", suppressWebviewMenu);
      window.removeEventListener("blur", flushSave);
      for (const unlisten of unlisteners) unlisten();
    };
  });
</script>

{#snippet splitter(pane: SplitPane)}
  <div
    class="splitter"
    class:dragging={draggingSplitter === pane}
    role="separator"
    aria-orientation="vertical"
    onpointerdown={(e) => dragSplitter(e, pane)}
    ondblclick={() => resetWidth(pane)}
  ></div>
{/snippet}

<div
  class="app"
  style:--text={s.textColor}
  style:--bg={s.backgroundColor}
  style:--panel={s.panelColor}
  style:--accent={s.accentColor}
  style:--heading={s.headingColor}
  style:--focus-alpha={s.focusAlpha}
  style:--page-title-size={`${s.pageTitleSize}px`}
>
  <aside class="tree-pane" class:focused={app.focus === "tree"} style:width="{s.treeWidth}px">
    <SectionStrip />
    <Tree />
  </aside>
  {@render splitter("tree")}
  <main class="main-pane">
    {#if app.view === "results"}
      <SearchBar />
      <div class="results-split">
        <Results />
        {@render splitter("peek")}
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
