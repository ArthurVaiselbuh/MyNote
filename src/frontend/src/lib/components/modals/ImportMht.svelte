<script lang="ts">
  import * as act from "../../actions";
  import { autofocus } from "../../autofocus";
  import { app } from "../../state/app.svelte";

  const files = $derived(app.importPreview ?? []);
  const importablePages = $derived(
    files.filter((f) => !f.error).reduce((n, f) => n + f.pages.length, 0),
  );
  const duplicatePages = $derived(
    files.flatMap((f) => f.pages).filter((p) => p.duplicate).length,
  );
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="560px" role="dialog">
    <div class="modal-title">Import OneNote export</div>
    <div class="import-files">
      {#each files as file (file.path)}
        <div class="import-file">
          <div class="import-head">
            <span class="import-section">{file.sectionName}</span>
            {#if file.error}
              <span class="import-warn">{file.error}</span>
            {:else}
              <span class="import-count">{file.pages.length} page(s) → new section</span>
              {#if file.sectionExists}
                <span class="import-warn">a section with this name already exists</span>
              {/if}
            {/if}
          </div>
          {#if !file.error}
            <ul class="import-pages">
              {#each file.pages as page, i (i)}
                <li>
                  {page.title}
                  {#if page.duplicate}
                    <span class="import-warn">· duplicate title</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    </div>
    {#if duplicatePages > 0}
      <div class="import-summary">
        {duplicatePages} page title(s) already exist in this notebook — importing keeps
        both copies.
      </div>
    {/if}
    <div class="modal-buttons">
      <button onclick={() => act.closeModal()}>Cancel</button>
      <button
        class="primary"
        use:autofocus
        disabled={importablePages === 0}
        onclick={() => void act.runMhtImport()}
      >
        Import {importablePages} page(s)
      </button>
    </div>
    <div class="hint">Each file becomes a new section · Enter imports · Esc cancels</div>
  </div>
</div>
