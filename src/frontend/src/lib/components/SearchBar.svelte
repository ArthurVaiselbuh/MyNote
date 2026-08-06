<script lang="ts">
  import * as act from "../actions";
  import type { SearchMode } from "../api";
  import { focusSelect } from "../autofocus";
  import { onRequest } from "../onRequest.svelte";
  import { app } from "../state/app.svelte";

  const MODES: SearchMode[] = ["fuzzy", "regex"];
  const DEBOUNCE_MS = 250;

  let input: HTMLInputElement | undefined = $state();
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  onRequest(
    () => app.searchFocusReq,
    () => {
      if (app.focus !== "search") return;
      requestAnimationFrame(() => focusSelect(input));
    },
  );

  function onInput() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void act.runSearch(), DEBOUNCE_MS);
  }

  function leaveToResults() {
    app.focus = "results";
    input?.blur();
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      clearTimeout(debounceTimer);
      void act.runSearch().then(leaveToResults);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      leaveToResults();
    }
  }

  function setMode(mode: SearchMode) {
    app.searchMode = mode;
    void act.runSearch();
    app.searchFocusReq++;
  }
</script>

<div class="search-bar pane-focusable" class:focused={app.focus === "search"} data-search-box>
  <input
    placeholder="Search notebook… (Enter to run)"
    bind:this={input}
    bind:value={app.searchQuery}
    oninput={onInput}
    onkeydown={keys}
    onfocus={() => (app.focus = "search")}
  />
  {#each MODES as mode}
    <button class="mode" class:active={app.searchMode === mode} onclick={() => setMode(mode)}>
      {mode}
    </button>
  {/each}
  {#if app.searchError}
    <span class="count error">{app.searchError}</span>
  {:else}
    <span class="count">{app.results.length} hit{app.results.length === 1 ? "" : "s"}</span>
  {/if}
</div>
