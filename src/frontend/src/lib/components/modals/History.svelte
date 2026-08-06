<script lang="ts">
  import * as act from "../../actions";
  import {
    api,
    type DeletedChild,
    type DeletedHistory,
    type DeletedItem,
    type PageRevision,
    type RevisionText,
  } from "../../api";
  import { diffText, type Span } from "../../diff";
  import { commandFor } from "../../keys/bindings";
  import { clampIndex, wrapIndex } from "../../listIndex";
  import { renderBody } from "../../markdown";
  import { app, type HistoryMode } from "../../state/app.svelte";
  import { countSubtree } from "../../treeUtils";

  const gitReady = $derived(!!app.git?.enabled && !!app.git?.repo);
  const onPageTab = $derived(app.historyTab === "page");
  const diffing = $derived(app.historyMode === "split" || app.historyMode === "inline");

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
    if (onPageTab && app.currentPageId && gitReady && loadedForPage !== app.currentPageId) {
      loadedForPage = app.currentPageId;
      void loadPageRevisions();
    }
  });

  function emptyRevision(): RevisionText {
    return { text: "", truncated: false, missing: false };
  }

  const textCache = new Map<string, RevisionText>();
  let base = $state.raw(emptyRevision());
  let target = $state.raw(emptyRevision());
  let textBusy = $state(false);
  // scoped to the content pane only — must never alias `pageError`, which
  // gates whether the rail itself renders (a failure loading one revision's
  // text must not take the whole revision list down with it)
  let textError = $state("");
  let textLoadSeq = 0;

  const tooLargeToDiff = $derived(base.truncated || target.truncated);

  async function fetchRev(pageId: string, sha: string): Promise<RevisionText> {
    const cached = textCache.get(sha);
    if (cached) return cached;
    const res = await api.revisionText(pageId, sha);
    textCache.set(sha, res);
    return res;
  }

  $effect(() => {
    if (!onPageTab || !gitReady) return;
    const pageId = app.currentPageId;
    if (!pageId) return;
    const baseSha = railSha(app.historyRevBase);
    const targetSha = railSha(app.historyRevSel);
    const seq = ++textLoadSeq;
    textBusy = true;
    textError = "";
    // sequential on purpose: two parallel invokes would spawn two git batches
    // that just queue on the backend's history gate anyway
    fetchRev(pageId, baseSha)
      .then(async (b) => [b, await fetchRev(pageId, targetSha)] as const)
      .then(([b, t]) => {
        if (seq !== textLoadSeq) return;
        base = b;
        target = t;
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
    if (tooLargeToDiff && diffing) app.historyMode = "text";
  });

  const diffRes = $derived(diffText(base.text, target.text));
  const anchors = $derived(app.historyMode === "inline" ? diffRes.unifiedAnchors : diffRes.anchors);

  let anchorPos = $state(0);
  $effect(() => {
    void diffRes;
    anchorPos = 0;
  });

  const currentAnchorRow = $derived(anchors[anchorPos] ?? -1);

  let contentEl: HTMLElement | undefined = $state();

  function jumpChange(dir: number) {
    if (anchors.length === 0) return;
    anchorPos = wrapIndex(anchorPos + dir, anchors.length);
    const rowIdx = anchors[anchorPos];
    requestAnimationFrame(() => {
      contentEl?.querySelector(`[data-row="${rowIdx}"]`)?.scrollIntoView({ block: "center" });
    });
  }

  function scrollContent(dir: number) {
    if (!contentEl) return;
    contentEl.scrollBy({ top: dir * contentEl.clientHeight * 0.85 });
  }

  function moveRailSel(delta: number) {
    app.historyRevSel = clampIndex(app.historyRevSel + delta, railCount);
  }
  function moveRailBase(delta: number) {
    app.historyRevBase = clampIndex(app.historyRevBase + delta, railCount);
  }
  function selectRail(idx: number, asBase: boolean) {
    if (asBase) app.historyRevBase = idx;
    else app.historyRevSel = idx;
  }

  // restoring while the text is still loading would write the empty
  // placeholder, and a missing revision has no content to restore to
  // (the page didn't exist yet)
  const canRestore = $derived(app.historyRevSel !== 0 && !textBusy && !target.missing);

  // ---------- deleted tab ----------

  interface FlatDeletedRow
    extends Pick<DeletedItem, "sha" | "id" | "title" | "at" | "sectionName" | "sectionExists"> {
    depth: number;
    isTop: boolean;
    count: number;
    resolved: boolean;
  }

  // every row of one deleted subtree shares the deletion's own facts
  type DeletedGroup = Omit<FlatDeletedRow, "id" | "title" | "depth" | "isTop" | "count">;

  function groupOf(item: DeletedItem): DeletedGroup {
    return {
      sha: item.sha,
      at: item.at,
      sectionName: item.sectionName,
      sectionExists: item.sectionExists,
      // a placement was found in the notebook.json history, so section and
      // nesting are known rather than guessed from the page's own H1
      resolved: item.sectionId !== null,
    };
  }

  function flattenDeleted(items: DeletedItem[]): FlatDeletedRow[] {
    const rows: FlatDeletedRow[] = [];
    for (const item of items) {
      const group = groupOf(item);
      const { id, title, pageCount: count } = item;
      rows.push({ ...group, id, title, count, depth: 0, isTop: true });
      appendChildren(item.children, group, 1, rows);
    }
    return rows;
  }

  function appendChildren(
    children: DeletedChild[],
    group: DeletedGroup,
    depth: number,
    rows: FlatDeletedRow[],
  ) {
    for (const child of children) {
      const { id, title } = child;
      rows.push({ ...group, id, title, count: countSubtree(child), depth, isTop: false });
      appendChildren(child.children, group, depth + 1, rows);
    }
  }

  let deletedHistory = $state<DeletedHistory | null>(null);
  let deletedBusy = $state(false);
  let deletedError = $state("");
  let deletedLoaded = false;
  let deletedMode = $state<"rendered" | "text">("rendered");

  const flatDeleted = $derived(deletedHistory ? flattenDeleted(deletedHistory.items) : []);
  const selectedDeleted: FlatDeletedRow | undefined = $derived(flatDeleted[app.historyDeletedSel]);

  async function loadDeleted() {
    deletedBusy = true;
    deletedError = "";
    try {
      deletedHistory = await api.deletedPages();
      if (app.historyDeletedSel >= flatDeleted.length) app.historyDeletedSel = 0;
    } catch (e) {
      deletedError = String(e);
      deletedHistory = null;
    } finally {
      deletedBusy = false;
    }
  }

  $effect(() => {
    if (!onPageTab && gitReady && !deletedLoaded) {
      deletedLoaded = true;
      void loadDeleted();
    }
  });

  let deletedPreviewText = $state("");
  let deletedPreviewBusy = $state(false);
  let deletedPreviewTruncated = $state(false);
  let deletedLoadSeq = 0;

  $effect(() => {
    if (onPageTab) return;
    const row = selectedDeleted;
    if (!row) {
      deletedPreviewText = "";
      return;
    }
    const seq = ++deletedLoadSeq;
    deletedPreviewBusy = true;
    api
      .deletedPageText(row.id, row.sha)
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
    app.historyDeletedSel = clampIndex(app.historyDeletedSel + delta, flatDeleted.length);
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
    if (onPageTab) {
      const pageId = app.currentPageId;
      if (!canRestore || !pageId) return;
      act.restoreRevision(pageId, target.text, railLabel(app.historyRevSel));
    } else if (selectedDeleted) {
      const { id, sha, title, count } = selectedDeleted;
      act.recoverDeletedPage(id, sha, title, count);
    }
  }

  function keys(e: KeyboardEvent) {
    // still mounted behind its own confirm dialog — that dialog owns the keyboard then
    if (app.modal !== "history") return;
    switch (commandFor("history", e)) {
      case "history.help":
        act.openHistoryHelp();
        break;
      case "history.selUp":
        if (onPageTab) moveRailSel(-1);
        else moveDeletedSel(-1);
        break;
      case "history.selDown":
        if (onPageTab) moveRailSel(1);
        else moveDeletedSel(1);
        break;
      case "history.baseUp":
        if (!onPageTab) return;
        moveRailBase(-1);
        break;
      case "history.baseDown":
        if (!onPageTab) return;
        moveRailBase(1);
        break;
      case "history.setBase":
        if (!onPageTab) return;
        app.historyRevBase = app.historyRevSel;
        break;
      case "history.modeSplit":
        if (!onPageTab) return;
        app.historyMode = "split";
        break;
      case "history.modeInline":
        if (!onPageTab) return;
        app.historyMode = "inline";
        break;
      case "history.modeRendered":
        if (onPageTab) app.historyMode = "rendered";
        else deletedMode = "rendered";
        break;
      case "history.modeText":
        if (onPageTab) app.historyMode = "text";
        else deletedMode = "text";
        break;
      case "history.cycleMode":
        if (onPageTab) {
          const order: HistoryMode[] = ["split", "inline", "rendered", "text"];
          app.historyMode = order[(order.indexOf(app.historyMode) + 1) % order.length];
        } else {
          deletedMode = deletedMode === "rendered" ? "text" : "rendered";
        }
        break;
      case "history.nextChange":
        if (!onPageTab || !diffing) return;
        jumpChange(1);
        break;
      case "history.prevChange":
        if (!onPageTab || !diffing) return;
        jumpChange(-1);
        break;
      case "history.switchTab":
        app.historyTab = onPageTab ? "deleted" : "page";
        break;
      case "history.restore":
        doRestore();
        break;
      default: {
        // the pane owns the keyboard outright, so PgUp/PgDn can't reach the
        // global dispatcher — page the diff here instead
        const scroll = commandFor("pane", e);
        if (!scroll) return;
        scrollContent(scroll === "pane.scrollUp" ? -1 : 1);
      }
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={keys} />

<!-- one line: .diff-cell is `white-space: pre-wrap`, so any indentation here would render -->
{#snippet spanRun(spans: Span[])}{#each spans as s, si (si)}{#if s.hl}<mark class="word">{s.text}</mark>{:else}{s.text}{/if}{/each}{/snippet}

<div class="history-layout" role="dialog" tabindex="-1">
    <div class="history-head">
      <div class="modal-title">History</div>

      <div class="import-modes">
        <button class="import-mode" class:active={onPageTab} onclick={() => (app.historyTab = "page")}>
          This page
        </button>
        <button class="import-mode" class:active={!onPageTab} onclick={() => (app.historyTab = "deleted")}>
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
    {:else if onPageTab}
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
              <button class:active={app.historyMode === "split"} disabled={tooLargeToDiff} onclick={() => (app.historyMode = "split")}>Side-by-side</button>
              <button class:active={app.historyMode === "inline"} disabled={tooLargeToDiff} onclick={() => (app.historyMode = "inline")}>Inline</button>
              <button class:active={app.historyMode === "rendered"} onclick={() => (app.historyMode = "rendered")}>Rendered</button>
              <button class:active={app.historyMode === "text"} onclick={() => (app.historyMode = "text")}>Text</button>
            </div>
            {#if diffing}
              <div class="history-stats">
                <span class="add">+{diffRes.stats.added}</span>
                <span class="del">−{diffRes.stats.removed}</span>
                <span class="change">~{diffRes.stats.changed}</span>
              </div>
            {/if}
            <button
              class="danger history-restore"
              disabled={!canRestore}
              onclick={doRestore}
            >
              Restore
            </button>
          </div>

          {#if diffRes.degraded && diffing}
            <div class="history-note">file too large for a precise line match — the changed region is shown as one block</div>
          {/if}
          {#if tooLargeToDiff}
            <div class="history-note">a revision is too large to diff — showing text mode</div>
          {/if}
          {#if target.missing}
            <div class="history-note">the page didn't exist at this revision</div>
          {:else if base.missing}
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
                      {#if row.left}{@render spanRun(row.left)}{/if}
                    </span>
                    <span class="diff-num">{row.rightNo ?? ""}</span>
                    <span class="diff-cell" class:tint-add={row.right !== null && row.kind !== "same"}>
                      {#if row.right}{@render spanRun(row.right)}{/if}
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
                      {@render spanRun(row.spans)}
                    </span>
                  </div>
                {/each}
              </div>
            {:else if app.historyMode === "rendered"}
              <div class="preview history-rendered">{@html renderBody(target.text)}</div>
            {:else}
              <pre class="history-raw">{target.text}</pre>
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
            <button class="danger history-restore" disabled={!selectedDeleted} onclick={doRestore}>
              Recover
            </button>
          </div>
          {#if deletedPreviewTruncated}
            <div class="history-note">this page is too large to show in full</div>
          {/if}
          <div class="history-content" bind:this={contentEl}>
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
