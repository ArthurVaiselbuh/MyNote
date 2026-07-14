<script lang="ts">
  import * as act from "../../actions";
  import { defaultSettings } from "../../api";
  import { app } from "../../state/app.svelte";

  function persist() {
    void act.persistSettings();
  }

  function resetColors() {
    app.settings.textColor = defaultSettings.textColor;
    app.settings.backgroundColor = defaultSettings.backgroundColor;
    app.settings.panelColor = defaultSettings.panelColor;
    app.settings.accentColor = defaultSettings.accentColor;
    app.settings.headingColor = defaultSettings.headingColor;
    app.settings.focusAlpha = defaultSettings.focusAlpha;
    persist();
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="480px" role="dialog">
    <div class="modal-title">Colors &amp; theme</div>

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
      <label for="set-heading">Heading color</label>
      <input id="set-heading" type="color" bind:value={app.settings.headingColor} onchange={persist} />
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
      <button onclick={resetColors}>Reset to defaults</button>
    </div>

    <div class="modal-buttons">
      <button class="primary" onclick={() => (app.modal = "settings")}>Back</button>
    </div>
  </div>
</div>
