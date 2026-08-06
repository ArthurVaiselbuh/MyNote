<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import * as act from "../../actions";
  import { api } from "../../api";
  import { hintOf } from "../../keys/bindings";
  import { app } from "../../state/app.svelte";

  const RELEASES_URL = "https://github.com/ArthurVaiselbuh/MyNote/releases";

  function persist() {
    void act.persistSettings();
  }

  const FALLBACK_INTERVAL_MINUTES = 20;

  function minutesOf(intervalSecs: number | undefined): number {
    return intervalSecs ? Math.max(1, Math.round(intervalSecs / 60)) : FALLBACK_INTERVAL_MINUTES;
  }

  let gitIntervalMinutes = $state(minutesOf(app.git?.intervalSecs));
  let version = $state("");

  onMount(() => {
    void getVersion().then((v) => (version = v));
    void api.getGitStatus().then((s) => {
      app.git = s;
      gitIntervalMinutes = minutesOf(s.intervalSecs);
    });
  });

  async function saveGitSnapshots(enabled: boolean) {
    try {
      app.git = await api.setGitSnapshots(enabled, gitIntervalMinutes * 60);
    } catch (err) {
      app.status = String(err);
    }
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="480px" role="dialog">
    <div class="modal-title">Settings</div>

    <div class="settings-section-label">General — every notebook</div>

    <div class="settings-row">
      <span>Colors &amp; theme</span>
      <button onclick={() => act.openModal("colors")}>Customize…</button>
    </div>
    <div class="settings-row">
      <span>Keyboard shortcuts</span>
      <button onclick={() => act.openModal("keybindings")}>Customize…</button>
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

    <div class="settings-row">
      <label for="set-tray">Minimize to tray instead of quitting(EXPERIMENTAL)</label>
      <input
        id="set-tray"
        type="checkbox"
        bind:checked={app.settings.minimizeToTray}
        onchange={persist}
      />
    </div>

    <div class="settings-row">
      <label for="set-startup">Start Mynote on startup</label>
      <input
        id="set-startup"
        type="checkbox"
        bind:checked={app.settings.startOnLogin}
        onchange={persist}
      />
    </div>
    {#if app.settings.startOnLogin && !app.settings.minimizeToTray}
      <div class="settings-path">
        Turn on minimize to tray as well to have MyNote start out of the way.
      </div>
    {/if}

    <div class="settings-row">
      <span>Welcome tour</span>
      <button onclick={() => act.openModal("welcome")}>Show again</button>
    </div>

    <div class="settings-group">
      <div class="settings-section-label">Current Notebook</div>
      <div class="settings-path">{app.root || "(none open)"}</div>

      <div class="settings-row">
        <span>Switch notebook{hintOf("notebook.open")}</span>
        <button
          onclick={() => {
            act.closeModal();
            void act.openNotebookModal();
          }}>Open…</button
        >
      </div>

      <div class="settings-row">
        <label for="set-git">Version history</label>
        <input
          id="set-git"
          type="checkbox"
          checked={app.git?.enabled ?? false}
          disabled={!app.git?.available}
          onchange={(e) => saveGitSnapshots(e.currentTarget.checked)}
        />
      </div>
      {#if app.git && !app.git.available}
        <div class="settings-path">
          git was not found on this machine — install it to enable version history.
        </div>
      {:else if app.git?.enabled}
        <div class="settings-row">
          <label for="set-git-interval">Snapshot interval (minutes)</label>
          <input
            id="set-git-interval"
            type="number"
            min="1"
            max="1440"
            step="1"
            bind:value={gitIntervalMinutes}
            onchange={() => saveGitSnapshots(true)}
          />
        </div>
      {/if}
      <div class="settings-row">
        <span>Reset AGENTS.md</span>
        <button onclick={() => act.overwriteAgentsMd()}>Overwrite…</button>
      </div>
    </div>

    <div class="modal-buttons">
      {#if version}
        <button
          class="settings-version"
          title="Open the releases page in your browser"
          onclick={() => act.openExternalLink(RELEASES_URL)}>v{version}</button
        >
      {/if}
      <button class="primary" onclick={() => act.closeModal()}>Done</button>
    </div>
  </div>
</div>
