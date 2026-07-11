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
  import ImportMht from "./lib/components/modals/ImportMht.svelte";
  import InsertHelper from "./lib/components/modals/InsertHelper.svelte";
  import OpenNotebook from "./lib/components/modals/OpenNotebook.svelte";
  import SectionPicker from "./lib/components/modals/SectionPicker.svelte";
  import SettingsModal from "./lib/components/modals/SettingsModal.svelte";

  const s = $derived(app.settings);

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
  style="--text:{s.textColor}; --bg:{s.backgroundColor}; --panel:{s.panelColor}; --accent:{s.accentColor}; --focus-alpha:{s.focusAlpha}"
>
  <aside class="tree-pane" class:focused={app.focus === "tree"}>
    <SectionStrip />
    <Tree />
  </aside>
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
  {#if app.modal === "sectionPicker"}<SectionPicker />{/if}
  {#if app.modal === "confirm"}<ConfirmDialog />{/if}
  {#if app.modal === "insert"}<InsertHelper />{/if}
  {#if app.modal === "importMht"}<ImportMht />{/if}
  {#if app.modal === "openNotebook"}<OpenNotebook />{/if}

  {#if app.status}
    <div class="status-toast">{app.status}</div>
  {/if}
</div>
