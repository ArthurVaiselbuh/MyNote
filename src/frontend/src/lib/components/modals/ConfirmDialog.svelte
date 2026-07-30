<script lang="ts">
  import * as act from "../../actions";
  import { autofocus } from "../../autofocus";
  import { app } from "../../state/app.svelte";

  function confirm() {
    const req = app.confirm;
    act.closeModal();
    req?.action();
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="420px" role="dialog">
    <div class="modal-title">Confirm</div>
    <div>{app.confirm?.message}</div>
    <div class="modal-buttons">
      <button onclick={() => act.escapeModal()}>Cancel</button>
      <button class="danger" use:autofocus onclick={confirm}
        >{app.confirm?.label ?? "Delete"}</button
      >
    </div>
    <div class="hint">Enter confirms · Esc cancels</div>
  </div>
</div>
