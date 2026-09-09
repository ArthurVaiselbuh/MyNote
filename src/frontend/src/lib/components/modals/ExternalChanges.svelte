<script lang="ts">
  import { app } from "../../state/app.svelte";
  import { resolveExternalChanges } from "../../externalChanges";
  import { autofocus } from "../../autofocus";

  const files = $derived([
    app.externalChanges?.[0] ? "notebook.json" : null,
    app.externalChanges?.[1] ? `${app.externalChanges[1]}.md` : null,
  ].filter(Boolean).join(" and "));
  const subject = $derived(app.externalChanges?.[0]
    ? (app.externalChanges[1] ? "notebook and open page" : "notebook") : "open page");
</script>

<div class="modal-backdrop">
  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="external-change-title" style:width="480px">
    <div class="modal-title" id="external-change-title">Files changed externally</div>
    <p>The {subject} changed outside MyNote. Saving is paused.</p>
    <p>Reload files from disk or overwrite with current data?</p>
    {#if app.externalChangeError}<p role="alert">Error loading {files}- {app.externalChangeError}</p>{/if}
    <div class="modal-buttons">
      <button disabled={app.externalResolution !== "idle"} onclick={() => resolveExternalChanges(false)}>Overwrite</button>
      <button disabled={app.externalResolution !== "idle"} use:autofocus onclick={() => resolveExternalChanges(true)}>Reload</button>
    </div>
  </div>
</div>
