<script lang="ts">
  import * as act from "../../actions";
  import { MOD_LABEL } from "../../keys/platform";
  import { app } from "../../state/app.svelte";

  function persist() {
    void act.persistSettings();
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="480px" role="dialog">
    <div class="modal-title">Settings</div>

    <div class="settings-row">
      <span>Colors &amp; theme</span>
      <button onclick={() => (app.modal = "colors")}>Customize…</button>
    </div>
    <div class="settings-row">
      <label for="set-scroll">Scroll speed</label>
      <input
        id="set-scroll"
        type="number"
        min="0.2"
        max="5"
        step="0.2"
        bind:value={app.settings.scrollSpeed}
        onchange={persist}
      />
    </div>

    <div class="settings-row">
      <label for="set-log">Log detail (applies after restart)</label>
      <select id="set-log" bind:value={app.settings.logLevel} onchange={persist}>
        <option value="off">Off</option>
        <option value="error">Errors only</option>
        <option value="warn">Warnings</option>
        <option value="info">Info</option>
        <option value="verbose">Verbose</option>
      </select>
    </div>

    <div class="modal-title" style="margin-top:14px">Notebook</div>
    <div class="settings-path">{app.root || "(none open)"}</div>
    <div class="settings-row">
      <span>Open a different notebook ({MOD_LABEL}+O)</span>
      <button
        onclick={() => {
          act.closeModal();
          void act.openNotebookModal();
        }}>Open…</button
      >
    </div>

    <div class="settings-row">
      <span>Welcome tour</span>
      <button onclick={() => (app.modal = "welcome")}>Show again</button>
    </div>

    <div class="modal-buttons">
      <button class="primary" onclick={() => act.closeModal()}>Done</button>
    </div>
  </div>
</div>
