<script lang="ts">
  import * as act from "../../actions";
  import { defaultSettings } from "../../api";
  import { MOD_LABEL } from "../../keys/platform";
  import { app } from "../../state/app.svelte";

  function persist() {
    void act.persistSettings();
  }

  function resetColors() {
    app.settings.textColor = defaultSettings.textColor;
    app.settings.backgroundColor = defaultSettings.backgroundColor;
    app.settings.panelColor = defaultSettings.panelColor;
    app.settings.accentColor = defaultSettings.accentColor;
    app.settings.focusAlpha = defaultSettings.focusAlpha;
    persist();
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="480px" role="dialog">
    <div class="modal-title">Settings</div>

    <div class="settings-row">
      <label for="set-text">Text color</label>
      <input id="set-text" type="color" bind:value={app.settings.textColor} onchange={persist} />
    </div>
    <div class="settings-row">
      <label for="set-bg">Background color</label>
      <input id="set-bg" type="color" bind:value={app.settings.backgroundColor} onchange={persist} />
    </div>
    <div class="settings-row">
      <label for="set-panel">Panel color</label>
      <input id="set-panel" type="color" bind:value={app.settings.panelColor} onchange={persist} />
    </div>
    <div class="settings-row">
      <label for="set-accent">Accent color</label>
      <input id="set-accent" type="color" bind:value={app.settings.accentColor} onchange={persist} />
    </div>
    <div class="settings-row">
      <label for="set-alpha">Focus highlight ({app.settings.focusAlpha.toFixed(2)})</label>
      <input
        id="set-alpha"
        type="range"
        min="0"
        max="1"
        step="0.05"
        bind:value={app.settings.focusAlpha}
        onchange={persist}
      />
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
      <span>Colors</span>
      <button onclick={resetColors}>Reset to defaults</button>
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
