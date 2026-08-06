<script lang="ts">
  import * as act from "../../actions";
  import type { PagePreview } from "../../api";
  import { autofocus } from "../../autofocus";
  import { app } from "../../state/app.svelte";
  import { countPages } from "../../treeUtils";

  const preview = $derived(app.importPreview);
  const canImport = $derived((preview?.pageCount ?? 0) > 0);

  let primaryBtn: HTMLButtonElement | undefined = $state();
  $effect(() => {
    if (canImport && primaryBtn) primaryBtn.focus();
  });

  function keys(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    if ((e.target as HTMLElement | null)?.tagName === "BUTTON") return;
    if (canImport) {
      e.preventDefault();
      void act.runImport();
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="modal-backdrop">
  <div class="modal import" style:width="560px" role="dialog" tabindex="-1">
    <div class="modal-title">Import</div>

    <div class="import-modes">
      <button
        class="import-mode"
        class:active={app.importMode === "md"}
        onclick={() => act.setImportMode("md")}
      >
        Markdown files / folder
      </button>
      <button
        class="import-mode"
        class:active={app.importMode === "mht"}
        onclick={() => act.setImportMode("mht")}
      >
        OneNote export (.mht)
      </button>
    </div>

    <div class="import-source">
      <button use:autofocus onclick={() => void act.chooseImportSource()}>
        {app.importMode === "md" ? "Choose folder…" : "Choose files…"}
      </button>
      {#if app.importSource}<span class="import-source-label">{app.importSource}</span>{/if}
    </div>

    <div class="import-hint">
      {#if app.importMode === "md"}
        Top-level folders become sections; nested folders become subpages. Files are
        copied into the notebook as new pages.
      {:else}
        Each OneNote file becomes a new section.
      {/if}
    </div>

    {#if app.importBusy}
      <div class="import-empty">Reading…</div>
    {:else if preview}
      {#if preview.sections.length === 0}
        <div class="import-empty">Nothing to import.</div>
      {:else}
        <div class="import-files">
          {#each preview.sections as section, i (i)}
            <div class="import-file">
              <div class="import-head">
                <span class="import-section">{section.name}</span>
                {#if section.error}
                  <span class="import-warn">{section.error}</span>
                {:else}
                  <span class="import-count">{countPages(section)} page(s) → new section</span>
                  {#if section.exists}
                    <span class="import-warn">a section with this name already exists</span>
                  {/if}
                {/if}
              </div>
              {#if !section.error && section.pages.length}
                <ul class="import-pages">
                  {@render pageRows(section.pages)}
                </ul>
              {/if}
            </div>
          {/each}
        </div>
        {#if preview.duplicateCount > 0}
          <div class="import-summary">
            {preview.duplicateCount} page title(s) already exist in this notebook — importing
            keeps both copies.
          </div>
        {/if}
      {/if}
    {/if}

    <div class="modal-buttons">
      <button onclick={() => act.closeModal()}>Cancel</button>
      <button
        bind:this={primaryBtn}
        class="primary"
        disabled={!canImport}
        onclick={() => void act.runImport()}
      >
        {canImport ? `Import ${preview?.pageCount} page(s)` : "Import"}
      </button>
    </div>
    <div class="hint">Choose a source · Enter imports · Esc cancels</div>
  </div>
</div>

{#snippet pageRows(pages: PagePreview[])}
  {#each pages as page, i (i)}
    <li>
      {page.title}
      {#if page.duplicate}<span class="import-warn">· duplicate title</span>{/if}
      {#if page.children.length}
        <ul class="import-pages">
          {@render pageRows(page.children)}
        </ul>
      {/if}
    </li>
  {/each}
{/snippet}
