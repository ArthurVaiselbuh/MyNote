<script lang="ts">
  import { closeContextMenu, contextMenu, runContextItem } from "../contextMenu.svelte";

  let menuEl: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);
  let placed = $state(false);

  const EDGE_GAP = 4;

  $effect(() => {
    if (!contextMenu.open) {
      placed = false;
      return;
    }
    void contextMenu.entries;
    if (!menuEl) return;
    const { width, height } = menuEl.getBoundingClientRect();
    left = Math.max(EDGE_GAP, Math.min(contextMenu.x, window.innerWidth - width - EDGE_GAP));
    top =
      contextMenu.y + height > window.innerHeight - EDGE_GAP
        ? Math.max(EDGE_GAP, contextMenu.y - height)
        : contextMenu.y;
    placed = true;
  });

  $effect(() => {
    if (!contextMenu.open) return;
    const dismissOutside = (e: PointerEvent) => {
      if (!menuEl?.contains(e.target as Node)) closeContextMenu();
    };
    window.addEventListener("pointerdown", dismissOutside, true);
    window.addEventListener("wheel", closeContextMenu, true);
    window.addEventListener("resize", closeContextMenu);
    window.addEventListener("blur", closeContextMenu);
    return () => {
      window.removeEventListener("pointerdown", dismissOutside, true);
      window.removeEventListener("wheel", closeContextMenu, true);
      window.removeEventListener("resize", closeContextMenu);
      window.removeEventListener("blur", closeContextMenu);
    };
  });
</script>

{#if contextMenu.open}
  <div
    class="context-menu"
    role="menu"
    tabindex="-1"
    bind:this={menuEl}
    style:left="{left}px"
    style:top="{top}px"
    style:visibility={placed ? "visible" : "hidden"}
  >
    {#each contextMenu.entries as entry, i}
      {#if entry === "separator"}
        <div class="sep"></div>
      {:else}
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
      {/if}
    {/each}
  </div>
{/if}
