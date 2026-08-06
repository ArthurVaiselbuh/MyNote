<script lang="ts">
  import { onMount } from "svelte";
  import * as act from "../actions";
  import { api, type SearchHit } from "../api";
  import { applyHighlights, setCurrentMatch } from "../highlight";
  import { wrapIndex } from "../listIndex";
  import { renderBody } from "../markdown";
  import { peekCtl } from "../paneCtl";
  import { searchRegex } from "../regex";
  import { app } from "../state/app.svelte";

  const LOAD_DELAY_MS = 120;

  const hit = $derived(app.results[app.resultsSel]);

  let body = $state("");
  let loadError = $state("");
  let containerEl: HTMLDivElement | undefined = $state();

  const html = $derived(body ? renderBody(body) : "");

  // plain, not $state: the loader reads and writes all of these, and tracking
  // them would make its effect depend on its own writes
  const cache = new Map<string, string>();
  let cachedFor: SearchHit[] | null = null;
  let loadedId: string | null = null;
  let loadToken = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function abandonLoad() {
    clearTimeout(timer);
    loadToken++;
  }

  $effect(() => {
    const target = hit;
    if (cachedFor !== app.results) {
      cache.clear();
      cachedFor = app.results;
      loadedId = null;
    }
    if (!target) {
      abandonLoad();
      loadedId = null;
      body = "";
      loadError = "";
      return;
    }
    if (target.pageId === loadedId) return;

    const cached = cache.get(target.pageId);
    if (cached !== undefined) {
      loadedId = target.pageId;
      body = cached;
      loadError = "";
      return;
    }

    // arrow-key traversal walks results faster than their pages can be read
    abandonLoad();
    const token = loadToken;
    const id = target.pageId;
    timer = setTimeout(async () => {
      try {
        const text = await api.readPage(id);
        cache.set(id, text);
        if (token !== loadToken) return;
        loadedId = id;
        body = text;
        loadError = "";
      } catch (e) {
        if (token !== loadToken) return;
        loadedId = id;
        body = "";
        loadError = String(e);
      }
    }, LOAD_DELAY_MS);
  });

  let matches: HTMLElement[] = [];
  let currentIdx = -1;
  let currentEl: HTMLElement | null = null;

  // landing on the first match rather than the hit's own line is deliberate:
  // it is what the reader searched for, and it holds even when the hit sits
  // outside the range the line number would have scrolled to.
  function renderInto(container: HTMLElement, markup: string) {
    container.innerHTML = markup;
    const prefill = act.resultFindPrefill();
    const re = searchRegex(prefill.text, prefill.regex);
    matches = re ? applyHighlights(container, re) : [];
    currentEl = null;
    currentIdx = -1;
    if (matches.length) goTo(0);
    else container.scrollTop = 0;
  }

  function goTo(idx: number) {
    currentIdx = idx;
    currentEl = setCurrentMatch(currentEl, matches[idx] ?? null);
  }

  function step(delta: number) {
    if (matches.length) goTo(wrapIndex(currentIdx + delta, matches.length));
  }

  // The peek writes this subtree itself instead of using `{@html}`: highlighting
  // rewrites text nodes underneath, which leaves Svelte's bookkeeping for its own
  // range stale and drops the body on the following update.
  $effect(() => {
    const markup = html;
    if (containerEl) renderInto(containerEl, markup);
  });

  onMount(() => {
    peekCtl.current = {
      openFind() {},
      closeFind: () => false,
      findOpen: () => false,
      findNext: () => step(1),
      findPrev: () => step(-1),
      scroller: () => containerEl ?? null,
    };
    return () => {
      peekCtl.current = null;
      abandonLoad();
    };
  });

  // a preview is not a place to navigate from — Enter on the result is
  function inertLinks(e: MouseEvent) {
    if ((e.target as HTMLElement).closest("a")) e.preventDefault();
  }
</script>

<aside class="peek" style:width="{app.settings.peekWidth}px">
  {#if hit}
    <div class="peek-head" title={`${hit.sectionName} › ${hit.title}`}>
      <span class="section">{hit.sectionName}</span> › {hit.title}
    </div>
    {#if loadError}
      <div class="peek-empty">{loadError}</div>
    {:else}
      <!-- `preview` carries the shared markdown + find-hit styling; the body is
           written by renderInto, never by Svelte -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="preview peek-body"
        id="peek-scroll"
        bind:this={containerEl}
        onclick={inertLinks}
      ></div>
    {/if}
  {:else}
    <div class="peek-empty">Select a result to preview it</div>
  {/if}
</aside>
