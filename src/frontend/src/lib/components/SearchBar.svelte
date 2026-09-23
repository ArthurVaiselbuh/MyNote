<script lang="ts">
  import { onDestroy } from "svelte";
  import * as act from "../actions";
  import type { SearchMode } from "../api";
  import { focusSelect } from "../autofocus";
  import { onRequest } from "../onRequest.svelte";
  import { app } from "../state/app.svelte";
  import { labelOf } from "../keys/bindings";

  const MODES: SearchMode[] = ["fuzzy", "regex"];
  const DEBOUNCE_MS = 250;

  let input: HTMLInputElement | undefined = $state();
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let sectionsButton: HTMLButtonElement | undefined = $state();
  let advancedButton: HTMLButtonElement | undefined = $state();
  const excludedSections = $derived(app.notebook?.sections.filter((section) =>
    app.searchPreferences.excludedSectionIds.includes(section.id)).length ?? 0);
  const excludedStrategies = $derived(app.searchPreferences.excludedStrategies.length);
  const exclusionsActive = $derived(excludedSections + excludedStrategies > 0);

  onDestroy(() => clearTimeout(debounceTimer));

  onRequest(
    () => app.searchAdvancedFocusReq,
    () => {
      if (app.focus === "search" && app.searchAdvanced) {
        requestAnimationFrame(() => sectionsButton?.focus());
      }
    },
  );

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

  function controlsKeys(event: KeyboardEvent) {
    if (event.key !== "Tab" || event.ctrlKey || event.metaKey || event.altKey) return;
    const controls = document.querySelectorAll<HTMLElement>(
      "#search-advanced button:not(:disabled), #search-advanced input:not(:disabled)",
    );
    if (event.shiftKey && event.currentTarget === controls[0]) {
      event.preventDefault();
      advancedButton?.focus();
    } else if (!event.shiftKey && (event.currentTarget === controls[controls.length - 1]
      || (event.currentTarget === advancedButton && !app.searchAdvanced))) {
      event.preventDefault();
      focusSelect(input);
    }
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
  <button
    class="advanced-toggle"
    bind:this={advancedButton}
    class:exclusions-active={exclusionsActive}
    style:--exclusion-color={app.settings.searchExclusionColor}
    aria-expanded={app.searchAdvanced}
    aria-controls="search-advanced"
    title={`${labelOf("results.advanced")} · ${excludedSections} sections and ${excludedStrategies} strategies excluded`}
    data-search-controls
    onkeydown={controlsKeys}
    onclick={() => {
      app.searchAdvanced = !app.searchAdvanced;
      app.focus = "search";
    }}
  >Advanced{exclusionsActive ? ` (${excludedSections + excludedStrategies})` : ""}</button>
  {#if app.searchError}
    <span class="count error">{app.searchError}</span>
  {:else}
    <span class="count">{app.results.length} hit{app.results.length === 1 ? "" : "s"}</span>
  {/if}
</div>

{#if app.searchAdvanced}
  <div id="search-advanced" class="search-advanced" data-search-controls>
    <button bind:this={sectionsButton} onkeydown={controlsKeys} onclick={() => act.openModal("searchSections")}>
      Excluded sections ({excludedSections})
    </button>
    <button onkeydown={controlsKeys} onclick={() => act.openModal("searchStrategies")}>
      Excluded strategies ({excludedStrategies})
    </button>
    <button
      disabled={!exclusionsActive || app.searchPreferencesSaving}
      onkeydown={controlsKeys}
      onclick={() => void act.updateSearchPreferences({
        ...app.searchPreferences, excludedSectionIds: [], excludedStrategies: [],
      })}
    >Clear exclusions</button>
    {#if app.searchPreferencesError}<span class="error" role="alert">{app.searchPreferencesError}</span>{/if}
  </div>
{/if}

<style>
  .advanced-toggle { border: 1px solid var(--guide); white-space: nowrap; }
  .advanced-toggle.exclusions-active {
    border-color: var(--exclusion-color);
    box-shadow: 0 0 0 1px var(--exclusion-color);
  }
  .search-advanced {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 12px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--guide);
    background: var(--panel);
  }
  .error { color: var(--error); }
</style>
