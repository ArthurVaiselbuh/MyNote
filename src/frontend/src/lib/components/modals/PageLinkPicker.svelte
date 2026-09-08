<script lang="ts">
  import * as act from "../../actions";
  import { api, type SearchHit } from "../../api";
  import { autofocusSelect } from "../../autofocus";
  import { clampIndex } from "../../listIndex";
  import { editorCtl } from "../../paneCtl";
  import { app } from "../../state/app.svelte";
  import { ancestorsOf } from "../../treeUtils";

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let selected = $state(0);
  let loading = $state(true);
  let error = $state("");
  let listEl = $state<HTMLDivElement>();

  $effect(() => {
    const search = query;
    let cancelled = false;
    loading = true;
    error = "";
    selected = 0;
    const timer = setTimeout(async () => {
      try {
        const results = await api.searchLinkTargets(search);
        if (!cancelled) hits = results.hits;
      } catch (failure) {
        if (!cancelled) {
          hits = [];
          error = String(failure);
        }
      } finally {
        if (!cancelled) loading = false;
      }
    }, search.trim() ? 150 : 0);
    return () => { cancelled = true; clearTimeout(timer); };
  });

  $effect(() => {
    (listEl?.children[selected] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
  });

  function pagePath(hit: SearchHit): string {
    const section = app.notebook?.sections.find((section) => section.id === hit.sectionId);
    const ancestors = section ? ancestorsOf(section.pages, hit.pageId) : [];
    return [hit.sectionName, ...ancestors.map((page) => page.title), hit.title].join(" / ");
  }

  function choose(hit: SearchHit) {
    if (loading) return;
    act.closeModal();
    editorCtl.current?.insertPageLink(hit.pageId, hit.title);
  }

  function keys(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      selected = clampIndex(selected + (event.key === "ArrowDown" ? 1 : -1), hits.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (hits[selected]) choose(hits[selected]);
    }
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="640px" role="dialog" aria-label="Link to page">
    <div class="modal-title">Link to page</div>
    <input aria-label="Find a page" placeholder="Find a page…" style="width:100%" bind:value={query} use:autofocusSelect onkeydown={keys} />
    <div class="page-list" bind:this={listEl} aria-busy={loading}>
      {#each hits as hit, index (hit.pageId)}
        <button class:selected={index === selected} disabled={loading} onclick={() => choose(hit)} onmousemove={() => (selected = index)}>
          <strong>{hit.title}</strong>
          <span class="path">{pagePath(hit)}</span>
          {#if query.trim() && hit.lineNo > 0}<span class="snippet">{hit.snippet}</span>{/if}
        </button>
      {/each}
    </div>
    <div class="hint" role="status">{loading ? "Searching…" : error || (hits.length ? `${hits.length} pages` : "No matching pages")}</div>
  </div>
</div>

<style>
  .page-list { max-height: 50vh; overflow-y: auto; margin-top: 12px; }
  .page-list button { display: flex; flex-direction: column; align-items: stretch; gap: 4px; width: 100%; text-align: left; padding: 10px 12px; border: 0; background: transparent; }
  .page-list button.selected { background: var(--select); outline: 1px solid var(--accent); outline-offset: -1px; }
  .path, .snippet { font-size: 12px; opacity: 0.7; overflow-wrap: anywhere; }
  strong { overflow-wrap: anywhere; }
  .snippet { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
