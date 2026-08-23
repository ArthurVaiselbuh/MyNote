<script lang="ts">
  import * as act from "../../actions";
  import { app, defaultSettings } from "../../state/app.svelte";

  function persist() {
    void act.persistSettings();
  }

  const COLOR_ROWS = [
    { key: "textColor", id: "set-text", label: "Text color" },
    { key: "backgroundColor", id: "set-bg", label: "Background color" },
    { key: "panelColor", id: "set-panel", label: "Panel color" },
    { key: "accentColor", id: "set-accent", label: "Accent color" },
    { key: "headingColor", id: "set-heading", label: "Heading color" },
  ] as const;

  function resetColors() {
    for (const row of COLOR_ROWS) app.settings[row.key] = defaultSettings[row.key];
    app.settings.focusAlpha = defaultSettings.focusAlpha;
    app.settings.pageTitleSize = defaultSettings.pageTitleSize;
    persist();
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="480px" role="dialog">
    <div class="modal-title">Colors &amp; theme</div>

    {#each COLOR_ROWS as row (row.key)}
      <div class="settings-row">
        <label for={row.id}>{row.label}</label>
        <input id={row.id} type="color" bind:value={app.settings[row.key]} onchange={persist} />
      </div>
    {/each}
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
      <label for="set-page-title-size">Page title size ({app.settings.pageTitleSize}px)</label>
      <input
        id="set-page-title-size"
        type="range"
        min="14"
        max="36"
        step="1"
        bind:value={app.settings.pageTitleSize}
        onchange={persist}
      />
    </div>
    <div class="settings-row">
      <button onclick={resetColors}>Reset to defaults</button>
    </div>

    <div class="modal-buttons">
      <button class="primary" onclick={() => act.escapeModal()}>Back</button>
    </div>
  </div>
</div>
