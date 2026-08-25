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
  let trash = $state<{ count: number; bytes: number } | null>(null);

  function refreshTrash() {
    void api.trashStats().then((s) => (trash = s));
  }

  onMount(() => {
    void getVersion().then((v) => (version = v));
    void api.getGitStatus().then((s) => {
      app.git = s;
      gitIntervalMinutes = minutesOf(s.intervalSecs);
    });
    refreshTrash();
    window.addEventListener("focus", refreshTrash);
    return () => window.removeEventListener("focus", refreshTrash);
  });

  async function saveGitSnapshots(enabled: boolean) {
    try {
      app.git = await api.setGitSnapshots(enabled, gitIntervalMinutes * 60);
    } catch (err) {
      app.status = String(err);
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    const units = ["KB", "MB", "GB"];
    let value = bytes / 1024;
    let i = 0;
    while (value >= 1024 && i < units.length - 1) {
      value /= 1024;
      i++;
    }
    return `${value.toFixed(1)} ${units[i]}`;
  }

  function trashLabel(): string {
    if (!trash) return "Show trash";
    return `Show trash (${trash.count} file${trash.count === 1 ? "" : "s"}, ${formatBytes(trash.bytes)})`;
  }

  async function openTrash() {
    try {
      await api.revealTrash();
    } catch (err) {
      app.status = String(err);
    }
  }

  function emptyTrash() {
    if (!trash?.count) return;
    const { count, bytes } = trash;
    act.askConfirm(
      `Permanently delete ${count} file${count === 1 ? "" : "s"} (${formatBytes(bytes)}) from trash? This cannot be undone.`,
      async () => {
        try {
          const removed = await api.emptyTrash();
          refreshTrash();
          app.status = `deleted ${removed} file${removed === 1 ? "" : "s"} from trash`;
        } catch (err) {
          app.status = String(err);
        }
      },
      "Empty trash",
      "settings",
    );
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
      <span>Keybindings</span>
      <button id="set-keybindings" onclick={() => act.openModal("keybindings")}>Customize…</button>
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
        <span>Deleted attachments and recovered files</span>
        <div class="settings-buttons">
          <button onclick={() => void openTrash()}>{trashLabel()}</button>
          {#if trash?.count}
            <button onclick={emptyTrash}>Empty…</button>
          {/if}
        </div>
      </div>
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
