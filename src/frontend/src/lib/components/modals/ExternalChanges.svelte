<script lang="ts">
  import { app } from "../../state/app.svelte";
  import { resolveExternalChanges } from "../../externalChanges";
  import { autofocus } from "../../autofocus";

  let busy = $state(false);
  let error = $state("");
  const subject = $derived(app.externalChanges?.[0]
    ? (app.externalChanges[1] ? "notebook and open page" : "notebook") : "open page");

  async function resolve(reload: boolean) {
    busy = true;
    error = "";
    try {
      await resolveExternalChanges(reload);
    } catch (failure) {
      error = String(failure);
    } finally {
      busy = false;
    }
  }
</script>

<div class="modal-backdrop">
  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="external-change-title" style:width="480px">
    <div class="modal-title" id="external-change-title">Files changed externally</div>
    <p>The {subject} changed outside MyNote. Saving is paused until you choose.</p>
    <p>Reload uses the changed files and discards your loaded data for those files. Overwrite replaces the changed files with the data currently loaded in MyNote.</p>
    {#if error}<p role="alert">{error}</p>{/if}
    <div class="modal-buttons">
      <button disabled={busy} onclick={() => resolve(false)}>Overwrite</button>
      <button disabled={busy} use:autofocus onclick={() => resolve(true)}>Reload</button>
    </div>
  </div>
</div>
