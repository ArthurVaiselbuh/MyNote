<script lang="ts">
  import * as act from "../../actions";
  import type { SearchStrategy } from "../../api";
  import { app } from "../../state/app.svelte";

  const strategies: { id: SearchStrategy; name: string; description: string }[] = [
    { id: "phrase", name: "Phrase", description: "The full query appears together, with word boundaries." },
    { id: "word", name: "Whole word", description: "Matched terms appear as whole words, without the full phrase." },
    { id: "partial", name: "Partial word", description: "At least one matched term appears only inside a longer word." },
    { id: "fuzzy", name: "Fuzzy", description: "At least one matched term needs an approximate match." },
  ];

  const sections = $derived(app.modal === "searchSections");
  let filter = $state("");
  const choices = $derived(sections
    ? (app.notebook?.sections ?? []).map((section) => ({ ...section, description: "" }))
      .filter((section) => section.name.toLowerCase().includes(filter.trim().toLowerCase()))
    : strategies);
  const excluded = $derived(sections
    ? app.searchPreferences.excludedSectionIds
    : app.searchPreferences.excludedStrategies);

  async function toggle(id: string, input: HTMLInputElement) {
    const next = excluded.includes(id) ? excluded.filter((value) => value !== id) : [...excluded, id];
    await act.updateSearchPreferences({
      ...app.searchPreferences,
      ...(sections ? { excludedSectionIds: next } : { excludedStrategies: next as SearchStrategy[] }),
    });
    if (input.isConnected && document.activeElement === document.body) input.focus();
  }

  function clear() {
    void act.updateSearchPreferences({
      ...app.searchPreferences,
      ...(sections ? { excludedSectionIds: [] } : { excludedStrategies: [] }),
    });
  }

  function focusFirstInput(node: HTMLElement) {
    requestAnimationFrame(() => node.querySelector<HTMLInputElement>("input")?.focus());
  }

  function containTab(event: KeyboardEvent) {
    if (event.key !== "Tab") return;
    const controls = Array.from((event.currentTarget as HTMLElement)
      .querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)'));
    const next = event.shiftKey ? controls.at(-1) : controls[0];
    const boundary = event.shiftKey ? controls[0] : controls.at(-1);
    if (document.activeElement === boundary) {
      event.preventDefault();
      next?.focus();
    }
  }
</script>

<div class="modal-backdrop">
  <div class="modal search-exclusions" role="dialog" aria-modal="true" aria-labelledby="exclusions-title" tabindex="-1" onkeydown={containTab} use:focusFirstInput>
    <div class="modal-title" id="exclusions-title">Excluded {sections ? "sections" : "search strategies"}</div>
    <p class="exclusion-hint">
      {sections ? "Checked sections are excluded from notebook search." : "Checked result categories are excluded from fuzzy search. Regex ignores this list."}
    </p>
    {#if sections}
      <input class="section-filter" aria-label="Filter sections" placeholder="Filter sections…" bind:value={filter} />
    {/if}
    <div class="exclusion-list">
      {#each choices as choice (choice.id)}
        <label class="exclusion-choice">
          <input type="checkbox" checked={excluded.includes(choice.id)} disabled={app.searchPreferencesSaving} onchange={(event) => void toggle(choice.id, event.currentTarget)} />
          <span><span>{choice.name}</span>{#if choice.description}<small>{choice.description}</small>{/if}</span>
        </label>
      {:else}
        <p class="exclusion-hint">No sections match.</p>
      {/each}
    </div>
    {#if app.searchPreferencesError}<p class="error" role="alert">{app.searchPreferencesError}</p>{/if}
    <div class="modal-buttons">
      <button disabled={!excluded.length || app.searchPreferencesSaving} onclick={clear}>Clear exclusions</button>
      <button class="primary" onclick={() => act.closeModal()}>Done</button>
    </div>
  </div>
</div>

<style>
  .search-exclusions { width: min(500px, calc(100vw - 32px)); }
  .exclusion-hint { color: var(--muted); margin: 0 0 12px; }
  .section-filter { width: 100%; margin-bottom: 10px; }
  .exclusion-list { max-height: 45vh; overflow-y: auto; }
  .exclusion-choice { display: flex; align-items: flex-start; gap: 10px; padding: 10px 4px; }
  .exclusion-choice input { flex: none; margin-top: 3px; }
  .exclusion-choice:has(input:focus-visible) {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
    border-radius: 4px;
  }
  .exclusion-choice span { overflow-wrap: anywhere; }
  small { display: block; color: var(--muted); margin-top: 3px; }
  .error { color: var(--error); }
</style>
