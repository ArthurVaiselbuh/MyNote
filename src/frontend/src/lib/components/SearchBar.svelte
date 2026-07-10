<script lang="ts">
  import * as act from "../actions";
  import { app } from "../state/app.svelte";

  let input: HTMLInputElement | undefined = $state();
  let lastReq = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (app.searchFocusReq !== lastReq) {
      lastReq = app.searchFocusReq;
      if (app.focus !== "search") return;
      requestAnimationFrame(() => {
        input?.focus();
        input?.select();
      });
    }
  });

  function onInput() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void act.runSearch(), 250);
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      clearTimeout(debounceTimer);
      void act.runSearch().then(() => {
        app.focus = "results";
        input?.blur();
      });
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      app.focus = "results";
      input?.blur();
    }
  }

  function setMode(mode: "fuzzy" | "regex") {
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
  <button class="mode" class:active={app.searchMode === "fuzzy"} onclick={() => setMode("fuzzy")}>
    fuzzy
  </button>
  <button class="mode" class:active={app.searchMode === "regex"} onclick={() => setMode("regex")}>
    regex
  </button>
  {#if app.searchError}
    <span class="count error">{app.searchError}</span>
  {:else}
    <span class="count">{app.results.length} hit{app.results.length === 1 ? "" : "s"}</span>
  {/if}
</div>
