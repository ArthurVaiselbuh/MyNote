<script lang="ts">
  import { closeContextMenu, contextMenu, isItem, runContextItem } from "../contextMenu.svelte";

  let menuEl: HTMLDivElement | undefined = $state();
  let pos = $state.raw<{ left: number; top: number } | null>(null);

  const EDGE_GAP = 4;

  $effect(() => {
    if (!contextMenu.open) {
      pos = null;
      return;
    }
    // the measured size below depends on the rendered entries, which Svelte
    // cannot see through getBoundingClientRect
    void contextMenu.entries;
    if (!menuEl) return;
    const { width, height } = menuEl.getBoundingClientRect();
    const flipAbove = contextMenu.y + height > window.innerHeight - EDGE_GAP;
    pos = {
      left: Math.max(EDGE_GAP, Math.min(contextMenu.x, window.innerWidth - width - EDGE_GAP)),
      top: flipAbove ? Math.max(EDGE_GAP, contextMenu.y - height) : contextMenu.y,
    };
  });

  $effect(() => {
    if (!contextMenu.open) return;
    const listeners = new AbortController();
    const { signal } = listeners;
    const dismissOutside = (e: PointerEvent) => {
      if (!menuEl?.contains(e.target as Node)) closeContextMenu();
    };
    window.addEventListener("pointerdown", dismissOutside, { capture: true, signal });
    window.addEventListener("wheel", closeContextMenu, { capture: true, signal });
    window.addEventListener("resize", closeContextMenu, { signal });
    window.addEventListener("blur", closeContextMenu, { signal });
    return () => listeners.abort();
  });
</script>

{#if contextMenu.open}
  <div
    class="context-menu"
    role="menu"
    tabindex="-1"
    bind:this={menuEl}
    style:left="{pos?.left ?? 0}px"
    style:top="{pos?.top ?? 0}px"
    style:visibility={pos ? "visible" : "hidden"}
  >
    {#each contextMenu.entries as entry, i}
      {#if isItem(entry)}
        <button
          class="item"
          class:danger={entry.danger}
          class:selected={i === contextMenu.sel}
          role="menuitem"
          onmousemove={() => (contextMenu.sel = i)}
          onclick={() => runContextItem(entry)}
        >
          <span class="label">{entry.label}</span>
          {#if entry.keys}<span class="keys">{entry.keys}</span>{/if}
        </button>
      {:else}
        <div class="sep"></div>
      {/if}
    {/each}
  </div>
{/if}
