<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as act from "../actions";
  import { modPressed } from "../keys/platform";
  import { renderBody } from "../markdown";
  import { previewFindCtl } from "../previewFindCtl";
  import type { FindPrefill } from "../state/app.svelte";

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

  function escapeRegExp(s: string): string {
    return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  function buildRegex(): RegExp | null {
    if (!query) return null;
    try {
      return new RegExp(regexMode ? query : escapeRegExp(query), caseSensitive ? "g" : "gi");
    } catch {
      return null;
    }
  }

  function clearHighlights() {
    if (!containerEl) return;
    const marks = containerEl.querySelectorAll("mark.find-hit");
    for (const m of marks) m.replaceWith(...Array.from(m.childNodes));
    containerEl.normalize();
  }

  // matches only within a single text node — a term split across inline
  // formatting (e.g. bold covering half the word) won't be found, the usual
  // limit of a per-text-node highlighter
  function applyHighlights(re: RegExp): HTMLElement[] {
    if (!containerEl) return [];
    const walker = document.createTreeWalker(containerEl, NodeFilter.SHOW_TEXT);
    const nodes: Text[] = [];
    let n: Node | null;
    while ((n = walker.nextNode())) nodes.push(n as Text);

    const found: HTMLElement[] = [];
    for (const node of nodes) {
      const text = node.textContent ?? "";
      re.lastIndex = 0;
      let m: RegExpExecArray | null;
      let last = 0;
      let hit = false;
      const frag = document.createDocumentFragment();
      while ((m = re.exec(text))) {
        if (m[0].length === 0) {
          re.lastIndex++;
          continue;
        }
        hit = true;
        if (m.index > last) frag.append(text.slice(last, m.index));
        const mark = document.createElement("mark");
        mark.className = "find-hit";
        mark.textContent = m[0];
        frag.append(mark);
        found.push(mark);
        last = m.index + m[0].length;
      }
      if (hit) {
        if (last < text.length) frag.append(text.slice(last));
        node.replaceWith(frag);
      }
    }
    return found;
  }

  function runSearch() {
    clearHighlights();
    matches = [];
    matchCount = 0;
    currentIdx = -1;
    displayIdx = -1;
    error = "";
    if (!open || !query) return;
    const re = buildRegex();
    if (!re) {
      error = "Invalid regex";
      return;
    }
    matches = applyHighlights(re);
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

  // switching pages while staying in preview keeps this component mounted —
  // close find rather than re-searching into content the query never applied to
  let mounted = false;
  $effect(() => {
    void body;
    if (mounted) open = false;
    mounted = true;
  });

  function keys(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) prev();
      else next();
    }
  }

  onMount(() => {
    previewFindCtl.current = {
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
    };
    return () => {
      previewFindCtl.current = null;
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
  <div class="preview" id="preview-scroll" bind:this={containerEl} onclick={onClick}>
    {@html html}
  </div>
</div>
