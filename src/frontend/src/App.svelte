<script lang="ts">
  import { onMount } from "svelte";
  import * as act from "./lib/actions";
  import { handleGlobal } from "./lib/keys/dispatch";
  import { app } from "./lib/state/app.svelte";
  import Editor from "./lib/components/Editor.svelte";
  import Results from "./lib/components/Results.svelte";
  import SearchBar from "./lib/components/SearchBar.svelte";
  import SectionStrip from "./lib/components/SectionStrip.svelte";
  import Tree from "./lib/components/Tree.svelte";
  import ConfirmDialog from "./lib/components/modals/ConfirmDialog.svelte";
  import HelpOverlay from "./lib/components/modals/HelpOverlay.svelte";
  import Import from "./lib/components/modals/Import.svelte";
  import ColorPicker from "./lib/components/modals/ColorPicker.svelte";
  import InsertHelper from "./lib/components/modals/InsertHelper.svelte";
  import OpenNotebook from "./lib/components/modals/OpenNotebook.svelte";
  import SectionPicker from "./lib/components/modals/SectionPicker.svelte";
  import SettingsModal from "./lib/components/modals/SettingsModal.svelte";
  import ColorsModal from "./lib/components/modals/ColorsModal.svelte";
  import Welcome from "./lib/components/Welcome.svelte";

  const s = $derived(app.settings);

  let splitterDragging = $state(false);

  function startTreeResize(e: PointerEvent) {
    const splitter = e.currentTarget as HTMLElement;
    const offset = app.settings.treeWidth - e.clientX;
    splitter.setPointerCapture(e.pointerId);
    splitterDragging = true;
    const move = (ev: PointerEvent) => act.setTreeWidth(ev.clientX + offset);
    const stop = () => {
      splitter.removeEventListener("pointermove", move);
      splitter.removeEventListener("pointerup", stop);
      splitter.removeEventListener("pointercancel", stop);
      splitterDragging = false;
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

    const flushSave = () => void act.saveNow();
    window.addEventListener("blur", flushSave);

    void act.boot();

    return () => {
      window.removeEventListener("keydown", handleGlobal, true);
      window.removeEventListener("wheel", wheel);
      window.removeEventListener("blur", flushSave);
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
    class:dragging={splitterDragging}
    role="separator"
    aria-orientation="vertical"
    onpointerdown={startTreeResize}
    ondblclick={resetTreeWidth}
  ></div>
  <main class="main-pane">
    {#if app.view === "results"}
      <SearchBar />
      <Results />
    {:else}
      <Editor />
    {/if}
  </main>

  {#if app.modal === "help"}<HelpOverlay />{/if}
  {#if app.modal === "settings"}<SettingsModal />{/if}
  {#if app.modal === "colors"}<ColorsModal />{/if}
  {#if app.modal === "sectionPicker"}<SectionPicker />{/if}
  {#if app.modal === "confirm"}<ConfirmDialog />{/if}
  {#if app.modal === "insert"}<InsertHelper />{/if}
  {#if app.modal === "colorPicker"}<ColorPicker />{/if}
  {#if app.modal === "import"}<Import />{/if}
  {#if app.modal === "openNotebook"}<OpenNotebook />{/if}

  <!-- always mounted: owns its own first-run trigger and visibility -->
  <Welcome />

  {#if app.status}
    <div class="status-toast">{app.status}</div>
  {/if}
</div>
