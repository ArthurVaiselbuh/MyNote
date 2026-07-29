<script lang="ts">
  import * as act from "../actions";
  import type { SearchHit } from "../api";
  import { app } from "../state/app.svelte";

  interface Segment {
    text: string;
    hl: boolean;
  }

  function segments(hit: SearchHit): Segment[] {
    const chars = Array.from(hit.snippet);
    const out: Segment[] = [];
    let pos = 0;
    const sorted = [...hit.ranges].sort((a, b) => a[0] - b[0]);
    for (const [start, end] of sorted) {
      if (start > pos) out.push({ text: chars.slice(pos, start).join(""), hl: false });
      if (end > start) out.push({ text: chars.slice(start, end).join(""), hl: true });
      pos = Math.max(pos, end);
    }
    if (pos < chars.length) out.push({ text: chars.slice(pos).join(""), hl: false });
    return out;
  }

  $effect(() => {
    const idx = app.resultsSel;
    requestAnimationFrame(() => {
      document
        .querySelector(`.results .hit[data-idx="${idx}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  });
</script>

<div class="results pane-focusable" id="results-scroll" class:focused={app.focus === "results"}>
  {#each app.results as hit, i (i)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="hit"
      class:selected={i === app.resultsSel}
      data-idx={i}
      onclick={() => act.openResult(i)}
      onmousemove={() => (app.resultsSel = i)}
    >
      <div class="meta">
        {hit.sectionName} › {hit.title}{hit.lineNo > 0 ? ` — line ${hit.lineNo}` : " — title"}
      </div>
      <div class="snippet">
        {#each segments(hit) as seg}{#if seg.hl}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
      </div>
    </div>
  {/each}
  {#if app.results.length === 0}
    <div class="empty">
      {app.searchQuery.trim()
        ? "No results"
        : 'Type a query above — keywords, "exact phrase", or regex'}
    </div>
  {/if}
</div>
