<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as act from "../actions";
  import { focusSelect } from "../autofocus";
  import { applyHighlights, clearHighlights, setCurrentMatch } from "../highlight";
  import { modPressed } from "../keys/platform";
  import { wrapIndex } from "../listIndex";
  import { renderBody } from "../markdown";
  import { openPreviewMenu } from "../menus";
  import { previewCtl } from "../paneCtl";
  import { previewPositionAt, scrollPreviewToBodyLine } from "../previewLines";
  import { attachmentFromHref, pageIdFromHref, searchRegex } from "../regex";
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
  let matchCount = $state(0);
  let currentIdx = $state(-1);
  // plain (non-reactive): showMatches() runs inside the search $effect and reads
  // both back in the same pass, so as $state the effect would depend on its own
  // writes and loop. currentIdx is write-only there, so it stays reactive for
  // the template.
  let matches: HTMLElement[] = [];
  let currentEl: HTMLElement | null = null;

  const findRe = $derived(open ? searchRegex(query, regexMode, caseSensitive) : null);
  const error = $derived(open && query && !findRe ? "Invalid regex" : "");

  function showMatches(re: RegExp | null) {
    if (containerEl) clearHighlights(containerEl);
    currentEl = null;
    currentIdx = -1;
    matches = re && containerEl ? applyHighlights(containerEl, re) : [];
    matchCount = matches.length;
    if (matches.length) goTo(0);
  }

  function goTo(idx: number) {
    currentIdx = idx;
    currentEl = setCurrentMatch(currentEl, matches[idx] ?? null);
  }

  function step(delta: number) {
    if (matches.length) goTo(wrapIndex(currentIdx + delta, matches.length));
  }

  $effect(() => {
    showMatches(findRe);
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
      if (anchor) scrollPreviewToBodyLine(containerEl, anchor.bodyLine, anchor.offsetRatio);
      else containerEl.scrollTop = pos.previewScrollTop;
      pos.previewScrollTop = containerEl.scrollTop;
    });
  }

  function currentMatch(): HTMLElement | null {
    return open ? currentEl : null;
  }

  function onFindKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      step(e.shiftKey ? -1 : 1);
    }
  }

  function openFind(prefill?: FindPrefill | null) {
    open = true;
    if (prefill?.text) {
      query = prefill.text;
      regexMode = prefill.regex;
      caseSensitive = false;
    }
    void tick().then(() => focusSelect(inputEl));
  }

  function closeFind() {
    const was = open;
    open = false;
    return was;
  }

  function anchor() {
    const pageId = app.currentPageId;
    if (!containerEl || !pageId) return null;
    const match = currentMatch();
    const position = previewPositionAt(containerEl, match);
    if (!position) return null;
    return { pageId, ...position, text: match?.textContent ?? undefined };
  }

  onMount(() => {
    previewCtl.current = {
      openFind,
      closeFind,
      findOpen: () => open,
      findNext: () => step(1),
      findPrev: () => step(-1),
      scroller: () => containerEl ?? null,
      anchor,
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
    const pageId = pageIdFromHref(href);
    if (pageId) {
      act.openPageById(pageId);
      return;
    }
    if (!modPressed(e)) return;
    if (attachmentFromHref(href)) act.revealAttachment(href);
    else act.openExternalLink(href);
  }
</script>

<div class="preview-wrap">
  {#if open}
    <div class="preview-find">
      <input placeholder="Find…" bind:this={inputEl} bind:value={query} onkeydown={onFindKeydown} />
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
        <span class="count">{matchCount ? `${currentIdx + 1}/${matchCount}` : "0 hits"}</span>
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
