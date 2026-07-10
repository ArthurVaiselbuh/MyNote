<script lang="ts">
  import * as act from "../actions";
  import { autofocusSelect } from "../autofocus";
  import { dndCancel, dndDown, dndMove, dndUp } from "../dnd";
  import { app } from "../state/app.svelte";

  let filterInput: HTMLInputElement | undefined = $state();
  let lastFilterReq = 0;

  const rows = $derived.by(() => act.visibleRows());

  $effect(() => {
    if (app.filterFocusReq !== lastFilterReq) {
      lastFilterReq = app.filterFocusReq;
      requestAnimationFrame(() => filterInput?.select());
    }
  });

  $effect(() => {
    const id = app.selectedId;
    if (!id) return;
    requestAnimationFrame(() => {
      document
        .querySelector(`.tree .row[data-id="${id}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  });

  function filterKeys(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      app.treeFilter = "";
      app.filterActive = false;
      app.focus = "tree";
      (e.currentTarget as HTMLInputElement).blur();
    } else if (e.key === "Enter" || e.key === "ArrowDown") {
      e.preventDefault();
      (e.currentTarget as HTMLInputElement).blur();
      app.focus = "tree";
      if (e.key === "ArrowDown") act.selectOffset(1);
      else if (!app.selectedId && rows.length) act.selectAndOpen(rows[0].node.id);
    }
  }

  function renameKeys(e: KeyboardEvent, id: string) {
    if (e.key === "Enter") {
      e.preventDefault();
      void act.commitRename(id, (e.currentTarget as HTMLInputElement).value);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      app.renamingId = null;
      app.focus = "tree";
    }
  }

  function rowClick(e: MouseEvent, id: string) {
    if ((e.target as HTMLElement).closest("input, button")) return;
    act.selectAndOpen(id);
    app.focus = "tree";
  }
</script>

<div
  class="tree"
  role="tree"
  onpointermove={dndMove}
  onpointerup={dndUp}
  onpointercancel={dndCancel}
>
  {#if app.filterActive || app.treeFilter}
    <input
      class="tree-filter"
      data-esc-local
      placeholder="filter pages…"
      bind:value={app.treeFilter}
      bind:this={filterInput}
      onkeydown={filterKeys}
      onfocus={() => (app.focus = "tree")}
    />
  {/if}
  {#each rows as row (row.node.id)}
    <div
      class="row"
      class:selected={row.node.id === app.selectedId}
      class:current={row.node.id === app.currentPageId}
      class:dragging={row.node.id === app.dragId}
      class:drop-before={app.dropTarget?.id === row.node.id && app.dropTarget.zone === "before"}
      class:drop-after={app.dropTarget?.id === row.node.id && app.dropTarget.zone === "after"}
      class:drop-inside={app.dropTarget?.id === row.node.id && app.dropTarget.zone === "inside"}
      data-id={row.node.id}
      role="treeitem"
      aria-selected={row.node.id === app.selectedId}
      onpointerdown={(e) => dndDown(e, row.node.id)}
      onclick={(e) => rowClick(e, row.node.id)}
      ondblclick={() => act.startRename(row.node.id)}
    >
      {#each Array.from({ length: row.depth }) as _}
        <span class="guide"></span>
      {/each}
      <button
        class="chev"
        class:hidden-chev={row.node.children.length === 0}
        tabindex="-1"
        onclick={(e) => {
          e.stopPropagation();
          void act.toggleExpand(row.node.id);
        }}
      >
        {row.node.expanded ? "▾" : "▸"}
      </button>
      {#if app.renamingId === row.node.id}
        <input
          class="rename"
          data-esc-local
          value={row.node.title}
          use:autofocusSelect
          onkeydown={(e) => renameKeys(e, row.node.id)}
          onblur={(e) => void act.commitRename(row.node.id, e.currentTarget.value)}
        />
      {:else}
        <span class="title">{row.node.title}</span>
      {/if}
    </div>
  {/each}
  {#if rows.length === 0}
    <div class="tree-empty">
      {app.treeFilter ? "No matches" : "No pages — Ctrl+N to create one"}
    </div>
  {/if}
</div>
