<script lang="ts">
  import * as act from "../../actions";
  import { api, type DeletedChild, type DeletedHistory, type DeletedItem, type PageRevision } from "../../api";
  import { diffText, type DiffResult } from "../../diff";
  import { chordKey, isHelpChord } from "../../keys/platform";
  import { renderBody } from "../../markdown";
  import { app, type HistoryMode } from "../../state/app.svelte";

  const gitReady = $derived(!!app.git?.enabled && !!app.git?.repo);

  // ---------- page tab ----------

  let pageRevs = $state<PageRevision[]>([]);
  let pageBusy = $state(false);
  let pageError = $state("");
  let loadedForPage: string | null = null;

  const railCount = $derived(1 + pageRevs.length);

  function railSha(idx: number): string {
    return idx === 0 ? "" : (pageRevs[idx - 1]?.sha ?? "");
  }
  function railLabel(idx: number): string {
    if (idx === 0) return "now (on disk)";
    const r = pageRevs[idx - 1];
    return r ? new Date(r.at * 1000).toLocaleString() : "";
  }
  function relativeAge(at: number): string {
    const mins = Math.max(0, Math.round((Date.now() / 1000 - at) / 60));
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins} min ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
    const days = Math.round(hours / 24);
    return `${days} day${days === 1 ? "" : "s"} ago`;
  }
  function railSub(idx: number): string {
    if (idx === 0) return "not in history yet";
    const r = pageRevs[idx - 1];
    if (!r) return "";
    const age = relativeAge(r.at);
    // auto-snapshot subjects only restate the timestamp already in the label
    return r.subject.startsWith("MyNote snapshot") ? age : `${age} · ${r.subject}`;
  }

  async function loadPageRevisions() {
    if (!app.currentPageId) return;
    pageBusy = true;
    pageError = "";
    try {
      pageRevs = await api.pageRevisions(app.currentPageId);
      // fresh page: compare against what's on disk now, pre-selecting the newest snapshot
      app.historyRevBase = 0;
      app.historyRevSel = pageRevs.length > 0 ? 1 : 0;
    } catch (e) {
      pageError = String(e);
      pageRevs = [];
    } finally {
      pageBusy = false;
    }
  }

  $effect(() => {
    if (
      app.historyTab === "page" &&
      app.currentPageId &&
      gitReady &&
      loadedForPage !== app.currentPageId
    ) {
      loadedForPage = app.currentPageId;
      void loadPageRevisions();
    }
  });

  const textCache = new Map<string, { text: string; truncated: boolean; missing: boolean }>();
  let baseText = $state("");
  let targetText = $state("");
  let baseTruncated = $state(false);
  let targetTruncated = $state(false);
  let baseMissing = $state(false);
  let targetMissing = $state(false);
  let textBusy = $state(false);
  // scoped to the content pane only — must never alias `pageError`, which
  // gates whether the rail itself renders (a failure loading one revision's
  // text must not take the whole revision list down with it)
  let textError = $state("");
  let textLoadSeq = 0;

  async function fetchRev(sha: string): Promise<{ text: string; truncated: boolean; missing: boolean }> {
    const cached = textCache.get(sha);
    if (cached) return cached;
    const res = await api.revisionText(app.currentPageId!, sha);
    textCache.set(sha, res);
    return res;
  }

  $effect(() => {
    if (app.historyTab !== "page" || !app.currentPageId || !gitReady) return;
    const baseSha = railSha(app.historyRevBase);
    const targetSha = railSha(app.historyRevSel);
    const seq = ++textLoadSeq;
    textBusy = true;
    textError = "";
    // sequential on purpose: two parallel invokes would spawn two git batches
    // that just queue on the backend's history gate anyway
    fetchRev(baseSha)
      .then(async (b) => [b, await fetchRev(targetSha)] as const)
      .then(([b, t]) => {
        if (seq !== textLoadSeq) return;
        baseText = b.text;
        baseTruncated = b.truncated;
        baseMissing = b.missing;
        targetText = t.text;
        targetTruncated = t.truncated;
        targetMissing = t.missing;
      })
      .catch((e) => {
        if (seq !== textLoadSeq) return;
        textError = String(e);
      })
      .finally(() => {
        if (seq === textLoadSeq) textBusy = false;
      });
  });

  // diffing a truncated blob would lie about what changed
  $effect(() => {
    if ((baseTruncated || targetTruncated) && (app.historyMode === "split" || app.historyMode === "inline")) {
      app.historyMode = "text";
    }
  });

  const diffRes = $derived.by((): DiffResult => diffText(baseText, targetText));

  let anchorPos = $state(0);
  $effect(() => {
    void diffRes;
    anchorPos = 0;
  });

  const currentAnchorRow = $derived.by((): number => {
    const anchors = app.historyMode === "inline" ? diffRes.unifiedAnchors : diffRes.anchors;
    return anchors[anchorPos] ?? -1;
  });

  let contentEl: HTMLElement | undefined = $state();

  function jumpChange(dir: number) {
    const anchors = app.historyMode === "inline" ? diffRes.unifiedAnchors : diffRes.anchors;
    if (anchors.length === 0) return;
    anchorPos = ((anchorPos + dir) % anchors.length + anchors.length) % anchors.length;
    const rowIdx = anchors[anchorPos];
    requestAnimationFrame(() => {
      contentEl?.querySelector(`[data-row="${rowIdx}"]`)?.scrollIntoView({ block: "center" });
    });
  }

  function moveRailSel(delta: number) {
    app.historyRevSel = Math.min(railCount - 1, Math.max(0, app.historyRevSel + delta));
  }
  function moveRailBase(delta: number) {
    app.historyRevBase = Math.min(railCount - 1, Math.max(0, app.historyRevBase + delta));
  }
  function selectRail(idx: number, asBase: boolean) {
    if (asBase) app.historyRevBase = idx;
    else app.historyRevSel = idx;
  }

  // ---------- deleted tab ----------

  interface FlatDeletedRow {
    sha: string;
    id: string;
    title: string;
    depth: number;
    isTop: boolean;
    count: number;
    at: number;
    sectionName: string | null;
    sectionExists: boolean;
    resolved: boolean;
  }

  function countChild(c: DeletedChild): number {
    return 1 + c.children.reduce((n, cc) => n + countChild(cc), 0);
  }

  function flattenDeleted(items: DeletedItem[]): FlatDeletedRow[] {
    const rows: FlatDeletedRow[] = [];
    for (const item of items) {
      rows.push({
        sha: item.sha,
        id: item.id,
        title: item.title,
        depth: 0,
        isTop: true,
        count: item.pageCount,
        at: item.at,
        sectionName: item.sectionName,
        sectionExists: item.sectionExists,
        resolved: item.resolved,
      });
      const walk = (children: DeletedChild[], depth: number) => {
        for (const c of children) {
          rows.push({
            sha: item.sha,
            id: c.id,
            title: c.title,
            depth,
            isTop: false,
            count: countChild(c),
            at: item.at,
            sectionName: item.sectionName,
            sectionExists: item.sectionExists,
            resolved: item.resolved,
          });
          walk(c.children, depth + 1);
        }
      };
      walk(item.children, 1);
    }
    return rows;
  }

  let deletedHistory = $state<DeletedHistory | null>(null);
  let deletedBusy = $state(false);
  let deletedError = $state("");
  let deletedLoaded = false;
  let deletedMode = $state<"rendered" | "text">("rendered");

  const flatDeleted = $derived(deletedHistory ? flattenDeleted(deletedHistory.items) : []);

  async function loadDeleted() {
    deletedBusy = true;
    deletedError = "";
    try {
      deletedHistory = await api.deletedPages();
      const rows = flattenDeleted(deletedHistory.items);
      if (app.historyDeletedSel >= rows.length) app.historyDeletedSel = 0;
    } catch (e) {
      deletedError = String(e);
      deletedHistory = null;
    } finally {
      deletedBusy = false;
    }
  }

  $effect(() => {
    if (app.historyTab === "deleted" && gitReady && !deletedLoaded) {
      deletedLoaded = true;
      void loadDeleted();
    }
  });

  let deletedPreviewText = $state("");
  let deletedPreviewBusy = $state(false);
  let deletedPreviewTruncated = $state(false);
  let deletedLoadSeq = 0;

  $effect(() => {
    if (app.historyTab !== "deleted") return;
    const row = flatDeleted[app.historyDeletedSel];
    if (!row) {
      deletedPreviewText = "";
      return;
    }
    const seq = ++deletedLoadSeq;
    deletedPreviewBusy = true;
    api
      .deletedPageText(row.sha, row.id)
      .then((res) => {
        if (seq !== deletedLoadSeq) return;
        deletedPreviewText = res.text;
        deletedPreviewTruncated = res.truncated;
      })
      .catch((e) => {
        if (seq !== deletedLoadSeq) return;
        deletedError = String(e);
      })
      .finally(() => {
        if (seq === deletedLoadSeq) deletedPreviewBusy = false;
      });
  });

  function moveDeletedSel(delta: number) {
    if (flatDeleted.length === 0) return;
    app.historyDeletedSel = Math.min(flatDeleted.length - 1, Math.max(0, app.historyDeletedSel + delta));
  }

  // ---------- shared ----------

  async function enableGit() {
    try {
      app.git = await api.setGitSnapshots(true);
    } catch (e) {
      app.status = String(e);
    }
  }

  function doRestore() {
    if (app.historyTab === "page") {
      // same guard as the restore button: Enter while the revision text is
      // still loading would "restore" the empty placeholder; a missing
      // revision has no content to restore to (the page didn't exist yet)
      if (app.historyRevSel === 0 || textBusy || targetMissing || !app.currentPageId) return;
      act.restoreRevision(app.currentPageId, targetText, railLabel(app.historyRevSel));
    } else {
      const row = flatDeleted[app.historyDeletedSel];
      if (!row) return;
      act.recoverDeletedPage(row.sha, row.id, row.title, row.count);
    }
  }

  function keys(e: KeyboardEvent) {
    // still mounted behind its own confirm dialog — that dialog owns the keyboard then
    if (app.modal !== "history") return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (isHelpChord(e)) {
      e.preventDefault();
      act.openHistoryHelp();
      return;
    }
    switch (chordKey(e)) {
      case "ArrowUp":
        e.preventDefault();
        if (e.shiftKey) {
          if (app.historyTab === "page") moveRailBase(-1);
        } else if (app.historyTab === "page") moveRailSel(-1);
        else moveDeletedSel(-1);
        return;
      case "ArrowDown":
        e.preventDefault();
        if (e.shiftKey) {
          if (app.historyTab === "page") moveRailBase(1);
        } else if (app.historyTab === "page") moveRailSel(1);
        else moveDeletedSel(1);
        return;
      case "b":
        if (app.historyTab === "page") {
          e.preventDefault();
          app.historyRevBase = app.historyRevSel;
        }
        return;
      case "1":
        if (app.historyTab === "page") {
          e.preventDefault();
          app.historyMode = "split";
        }
        return;
      case "2":
        if (app.historyTab === "page") {
          e.preventDefault();
          app.historyMode = "inline";
        }
        return;
      case "3":
        e.preventDefault();
        if (app.historyTab === "page") app.historyMode = "rendered";
        else deletedMode = "rendered";
        return;
      case "4":
        e.preventDefault();
        if (app.historyTab === "page") app.historyMode = "text";
        else deletedMode = "text";
        return;
      case "v": {
        e.preventDefault();
        if (app.historyTab === "page") {
          const order: HistoryMode[] = ["split", "inline", "rendered", "text"];
          app.historyMode = order[(order.indexOf(app.historyMode) + 1) % order.length];
        } else {
          deletedMode = deletedMode === "rendered" ? "text" : "rendered";
        }
        return;
      }
      case "n":
        if (app.historyTab === "page" && (app.historyMode === "split" || app.historyMode === "inline")) {
          e.preventDefault();
          jumpChange(1);
        }
        return;
      case "p":
        if (app.historyTab === "page" && (app.historyMode === "split" || app.historyMode === "inline")) {
          e.preventDefault();
          jumpChange(-1);
        }
        return;
      case "F3":
        if (app.historyTab === "page") {
          e.preventDefault();
          jumpChange(e.shiftKey ? -1 : 1);
        }
        return;
      case "Tab":
        e.preventDefault();
        app.historyTab = app.historyTab === "page" ? "deleted" : "page";
        return;
      case "Enter":
      case "r":
        e.preventDefault();
        doRestore();
        return;
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="history-layout" role="dialog" tabindex="-1">
    <div class="history-head">
      <div class="modal-title">History</div>

      <div class="import-modes">
        <button class="import-mode" class:active={app.historyTab === "page"} onclick={() => (app.historyTab = "page")}>
          This page
        </button>
        <button
          class="import-mode"
          class:active={app.historyTab === "deleted"}
          onclick={() => (app.historyTab = "deleted")}
        >
          Deleted pages
        </button>
      </div>

      <div class="history-head-actions">
        <button onclick={act.openHistoryHelp}>Shortcuts (?)</button>
        <button onclick={act.closeModal}>Close (Esc)</button>
      </div>
    </div>

    {#if !gitReady}
      <div class="history-empty">
        <p>Version history isn't turned on for this notebook yet.</p>
        <button class="primary" onclick={() => void enableGit()}>Enable version history</button>
      </div>
    {:else if app.historyTab === "page"}
      <div class="history-body">
        <div class="history-rail">
          {#if pageBusy}
            <div class="history-note">Loading…</div>
          {:else if pageError}
            <div class="history-note">{pageError}</div>
          {:else}
            {#each Array(railCount) as _, i (i)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="picker-item"
                class:selected={i === app.historyRevSel}
                class:base={i === app.historyRevBase}
                onclick={(e) => selectRail(i, e.shiftKey)}
              >
                <span>
                  {railLabel(i)}
                  {#if i === app.historyRevBase}<span class="rev-tag">base</span>{/if}
                  {#if i === app.historyRevSel}<span class="rev-tag accent">selected</span>{/if}
                </span>
                <span class="sub">{railSub(i)}</span>
              </div>
            {/each}
          {/if}
        </div>

        <div class="history-pane">
          <div class="history-toolbar">
            <div class="history-modes">
              <button class:active={app.historyMode === "split"} disabled={baseTruncated || targetTruncated} onclick={() => (app.historyMode = "split")}>Side-by-side</button>
              <button class:active={app.historyMode === "inline"} disabled={baseTruncated || targetTruncated} onclick={() => (app.historyMode = "inline")}>Inline</button>
              <button class:active={app.historyMode === "rendered"} onclick={() => (app.historyMode = "rendered")}>Rendered</button>
              <button class:active={app.historyMode === "text"} onclick={() => (app.historyMode = "text")}>Text</button>
            </div>
            {#if app.historyMode === "split" || app.historyMode === "inline"}
              <div class="history-stats">
                <span class="add">+{diffRes.stats.added}</span>
                <span class="del">−{diffRes.stats.removed}</span>
                <span class="change">~{diffRes.stats.changed}</span>
              </div>
            {/if}
            <button
              class="danger history-restore"
              disabled={app.historyRevSel === 0 || textBusy || targetMissing}
              onclick={doRestore}
            >
              Restore
            </button>
          </div>

          {#if diffRes.degraded && (app.historyMode === "split" || app.historyMode === "inline")}
            <div class="history-note">file too large for a precise line match — the changed region is shown as one block</div>
          {/if}
          {#if baseTruncated || targetTruncated}
            <div class="history-note">a revision is too large to diff — showing text mode</div>
          {/if}
          {#if targetMissing}
            <div class="history-note">the page didn't exist at this revision</div>
          {:else if baseMissing}
            <div class="history-note">the page didn't exist at the base revision</div>
          {/if}

          <div class="history-content" bind:this={contentEl}>
            {#if textBusy}
              <div class="history-note">Loading…</div>
            {:else if textError}
              <div class="history-note">{textError}</div>
            {:else if app.historyMode === "split"}
              <div class="diff split">
                <div class="diff-head">
                  <span></span>
                  <span>{railLabel(app.historyRevBase)}</span>
                  <span></span>
                  <span>{railLabel(app.historyRevSel)}</span>
                </div>
                {#each diffRes.rows as row, i (i)}
                  <div class="diff-row" class:cursor={i === currentAnchorRow} data-row={i}>
                    <span class="diff-num">{row.leftNo ?? ""}</span>
                    <span class="diff-cell" class:tint-del={row.left !== null && row.kind !== "same"}>
                      {#if row.left}{#each row.left as s, si (si)}{#if s.hl}<mark class="word">{s.text}</mark>{:else}{s.text}{/if}{/each}{/if}
                    </span>
                    <span class="diff-num">{row.rightNo ?? ""}</span>
                    <span class="diff-cell" class:tint-add={row.right !== null && row.kind !== "same"}>
                      {#if row.right}{#each row.right as s, si (si)}{#if s.hl}<mark class="word">{s.text}</mark>{:else}{s.text}{/if}{/each}{/if}
                    </span>
                  </div>
                {/each}
              </div>
            {:else if app.historyMode === "inline"}
              <div class="diff inline">
                {#each diffRes.unified as row, i (i)}
                  <div
                    class="diff-row"
                    class:cursor={i === currentAnchorRow}
                    class:tint-del={row.kind === "del"}
                    class:tint-add={row.kind === "add"}
                    data-row={i}
                  >
                    <span class="diff-num">{row.leftNo ?? ""}</span>
                    <span class="diff-num">{row.rightNo ?? ""}</span>
                    <span class="diff-marker">{row.kind === "del" ? "−" : row.kind === "add" ? "+" : ""}</span>
                    <span class="diff-cell">
                      {#each row.spans as s, si (si)}{#if s.hl}<mark class="word">{s.text}</mark>{:else}{s.text}{/if}{/each}
                    </span>
                  </div>
                {/each}
              </div>
            {:else if app.historyMode === "rendered"}
              <div class="preview history-rendered">{@html renderBody(targetText)}</div>
            {:else}
              <pre class="history-raw">{targetText}</pre>
            {/if}
          </div>
        </div>
      </div>
    {:else}
      <div class="history-body">
        <div class="history-rail">
          {#if deletedBusy}
            <div class="history-note">Loading…</div>
          {:else if deletedError}
            <div class="history-note">{deletedError}</div>
          {:else if flatDeleted.length === 0}
            <div class="history-note">No pages have been removed since snapshots were turned on.</div>
          {:else}
            {#each flatDeleted as row, i (i)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="picker-item deleted-row"
                style:padding-left="{10 + row.depth * 14}px"
                class:selected={i === app.historyDeletedSel}
                onclick={() => (app.historyDeletedSel = i)}
              >
                <span>{row.title}</span>
                <span class="sub">
                  {#if row.isTop}
                    {new Date(row.at * 1000).toLocaleDateString()}
                    {row.sectionName ? `· ${row.sectionName}` : ""}
                    {row.count > 1 ? `· ${row.count} pages` : ""}
                    {#if !row.sectionExists && row.sectionName}<span class="rev-tag">section gone</span>{/if}
                    {#if !row.resolved}<span class="rev-tag">title only</span>{/if}
                  {:else}
                    nested page
                  {/if}
                </span>
              </div>
            {/each}
          {/if}
        </div>

        <div class="history-pane">
          <div class="history-toolbar">
            <div class="history-modes">
              <button class:active={deletedMode === "rendered"} onclick={() => (deletedMode = "rendered")}>Rendered</button>
              <button class:active={deletedMode === "text"} onclick={() => (deletedMode = "text")}>Text</button>
            </div>
            <button class="danger history-restore" disabled={!flatDeleted[app.historyDeletedSel]} onclick={doRestore}>
              Recover
            </button>
          </div>
          {#if deletedPreviewTruncated}
            <div class="history-note">this page is too large to show in full</div>
          {/if}
          <div class="history-content">
            {#if deletedPreviewBusy}
              <div class="history-note">Loading…</div>
            {:else if deletedMode === "rendered"}
              <div class="preview history-rendered">{@html renderBody(deletedPreviewText)}</div>
            {:else}
              <pre class="history-raw">{deletedPreviewText}</pre>
            {/if}
          </div>
        </div>
      </div>
    {/if}

</div>
