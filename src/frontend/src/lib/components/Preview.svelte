<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as act from "../actions";
  import { applyHighlights, clearHighlights } from "../highlight";
  import { modPressed } from "../keys/platform";
  import { renderBody } from "../markdown";
  import { openPreviewMenu } from "../menus";
  import { previewCtl } from "../previewCtl";
  import { previewPositionAt, scrollPreviewToBodyLine } from "../previewLines";
  import { escapeRegExp } from "../regex";
  import { app, type FindPrefill } from "../state/app.svelte";
  import { takeModeAnchor, viewPosChanged, viewPosOf } from "../viewPos";

  let { body }: { body: string } = $props();

  const html = $derived(renderBody(body));

  let containerEl: HTMLDivElement | undefined = $state();
  let inputEl: HTMLInputElement | undefined = $state();

  let open = $state(false);
  let query = $state("");
  let caseSensitive = $state(false);
  let regexMode = $state(false);
  let error = $state("");
  let matchCount = $state(0);
  // plain (non-reactive): goTo() reads the previous value to unhighlight it,
  // then writes the new one — as $state that read+write in the same pass
  // that runSearch() runs under would make the search $effect depend on and
  // mutate its own dependency, looping. displayIdx below is the read-only
  // reactive mirror the template renders from.
  let currentIdx = -1;
  let displayIdx = $state(-1);
  let matches: HTMLElement[] = [];

  function buildRegex(): RegExp | null {
    if (!query) return null;
    try {
      return new RegExp(regexMode ? query : escapeRegExp(query), caseSensitive ? "g" : "gi");
    } catch {
      return null;
    }
  }

  function runSearch() {
    if (containerEl) clearHighlights(containerEl);
    matches = [];
    matchCount = 0;
    currentIdx = -1;
    displayIdx = -1;
    error = "";
    if (!open || !query || !containerEl) return;
    const re = buildRegex();
    if (!re) {
      error = "Invalid regex";
      return;
    }
    matches = applyHighlights(containerEl, re);
    matchCount = matches.length;
    if (matches.length) goTo(0);
  }

  function goTo(idx: number) {
    matches[currentIdx]?.classList.remove("current");
    currentIdx = idx;
    displayIdx = idx;
    const m = matches[currentIdx];
    if (m) {
      m.classList.add("current");
      m.scrollIntoView({ block: "center" });
    }
  }

  function next() {
    if (matches.length) goTo((currentIdx + 1) % matches.length);
  }
  function prev() {
    if (matches.length) goTo((currentIdx - 1 + matches.length) % matches.length);
  }

  $effect(() => {
    void query;
    void caseSensitive;
    void regexMode;
    void open;
    runSearch();
  });

  // switching pages while staying in preview keeps this component mounted, so a
  // page change shows up as a new body: close find rather than re-searching into
  // content the query never applied to, and resume that page's own scroll
  let shownPageId: string | null = null;
  let shownBody: string | null = null;
  $effect(() => {
    if (body === shownBody) return;
    if (shownBody !== null) open = false;
    // adopt the new page as soon as it is current, so scroll events from the
    // re-render are recorded against the page they belong to; the position is
    // only restored once its body has actually arrived
    shownPageId = app.currentPageId;
    shownBody = body;
    restoreScroll(shownPageId);
  });

  function rememberScroll() {
    if (!containerEl || !shownPageId) return;
    viewPosOf(shownPageId).previewScrollTop = containerEl.scrollTop;
    viewPosChanged(shownPageId);
  }

  function restoreScroll(pageId: string | null) {
    if (!pageId) return;
    const anchor = takeModeAnchor(pageId);
    requestAnimationFrame(() => {
      // a find match owns the scroll — opening a search result lands here
      if (!containerEl || currentMatch()) return;
      const pos = viewPosOf(pageId);
      if (anchor) scrollPreviewToBodyLine(containerEl, anchor.bodyLine, anchor.offsetFromTop);
      else containerEl.scrollTop = pos.previewScrollTop;
      pos.previewScrollTop = containerEl.scrollTop;
    });
  }

  function currentMatch(): HTMLElement | null {
    return (open && matches[currentIdx]) || null;
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) prev();
      else next();
    }
  }

  onMount(() => {
    previewCtl.current = {
      openFind(prefill?: FindPrefill | null) {
        open = true;
        if (prefill?.text) {
          query = prefill.text;
          regexMode = prefill.regex;
          caseSensitive = false;
        }
        void tick().then(() => {
          inputEl?.focus();
          inputEl?.select();
        });
      },
      closeFind() {
        const was = open;
        open = false;
        return was;
      },
      findOpen: () => open,
      findNext: next,
      findPrev: prev,
      anchor() {
        const pageId = app.currentPageId;
        if (!containerEl || !pageId) return null;
        const match = currentMatch();
        const position = previewPositionAt(containerEl, match);
        if (!position) return null;
        return { pageId, ...position, text: match?.textContent ?? undefined };
      },
    };
    return () => {
      previewCtl.current = null;
    };
  });

  function onClick(e: MouseEvent) {
    const link = (e.target as HTMLElement).closest("a");
    if (!link) return;
    e.preventDefault();
    const href = link.getAttribute("href") ?? "";
    const match = href.match(/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.md$/i);
    if (match) act.openPageById(match[1]);
    else if (modPressed(e)) act.openExternalLink(href);
  }
</script>

<div class="preview-wrap">
  {#if open}
    <div class="preview-find">
      <input placeholder="Find…" bind:this={inputEl} bind:value={query} onkeydown={keys} />
      <button
        class="mode"
        class:active={caseSensitive}
        title="Case sensitive"
        onclick={() => (caseSensitive = !caseSensitive)}
      >
        Aa
      </button>
      <button
        class="mode"
        class:active={regexMode}
        title="Regex"
        onclick={() => (regexMode = !regexMode)}
      >
        .*
      </button>
      {#if error}
        <span class="count error">{error}</span>
      {:else}
        <span class="count">{matchCount ? `${displayIdx + 1}/${matchCount}` : "0 hits"}</span>
      {/if}
    </div>
  {/if}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="preview"
    id="preview-scroll"
    bind:this={containerEl}
    onclick={onClick}
    onscroll={rememberScroll}
    oncontextmenu={openPreviewMenu}
  >
    {@html html}
  </div>
</div>
