<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import * as act from "../../actions";
  import { api, type GitStatus } from "../../api";
  import { hintOf } from "../../keys/bindings";
  import { app } from "../../state/app.svelte";

  const RELEASES_URL = "https://github.com/ArthurVaiselbuh/MyNote/releases";

  function persist() {
    void act.persistSettings();
  }

  let gitStatus = $state<GitStatus | null>(null);
  let gitIntervalMinutes = $state(60);
  let version = $state("");

  onMount(() => {
    void getVersion().then((v) => (version = v));
    void api.getGitStatus().then((s) => {
      gitStatus = s;
      app.git = s;
      gitIntervalMinutes = Math.max(1, Math.round(s.intervalSecs / 60));
    });
  });

  async function toggleGit(e: Event) {
    const enabled = (e.currentTarget as HTMLInputElement).checked;
    try {
      gitStatus = await api.setGitSnapshots(enabled, gitIntervalMinutes * 60);
      app.git = gitStatus;
    } catch (err) {
      app.status = String(err);
    }
  }

  async function changeGitInterval() {
    if (!gitStatus?.enabled) return;
    try {
      gitStatus = await api.setGitSnapshots(true, gitIntervalMinutes * 60);
      app.git = gitStatus;
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
      <button onclick={() => (app.modal = "colors")}>Customize…</button>
    </div>
    <div class="settings-row">
      <span>Keyboard shortcuts</span>
      <button onclick={() => (app.modal = "keybindings")}>Customize…</button>
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
      <button onclick={() => (app.modal = "welcome")}>Show again</button>
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
          checked={gitStatus?.enabled ?? false}
          disabled={!gitStatus?.available}
          onchange={toggleGit}
        />
      </div>
      {#if gitStatus && !gitStatus.available}
        <div class="settings-path">
          git was not found on this machine — install it to enable version history.
        </div>
      {:else if gitStatus?.enabled}
        <div class="settings-row">
          <label for="set-git-interval">Snapshot interval (minutes)</label>
          <input
            id="set-git-interval"
            type="number"
            min="1"
            max="1440"
            step="1"
            bind:value={gitIntervalMinutes}
            onchange={changeGitInterval}
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
