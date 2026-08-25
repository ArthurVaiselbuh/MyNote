<script lang="ts">
  import * as act from "../actions";
  import { autofocusSelect } from "../autofocus";
  import { dndCancel, dndDown, dndMove, dndUp } from "../dnd";
  import { labelOf } from "../keys/bindings";
  import { openTreeMenu } from "../menus";
  import { onRequest } from "../onRequest.svelte";
  import { app } from "../state/app.svelte";

  type InputKeyEvent = KeyboardEvent & { currentTarget: HTMLInputElement };

  let treeEl: HTMLDivElement | undefined = $state();
  let filterInput: HTMLInputElement | undefined = $state();

  const rows = $derived(act.visibleRows());

  onRequest(
    () => app.filterFocusReq,
    () => requestAnimationFrame(() => filterInput?.select()),
  );

  $effect(() => {
    const id = app.selectedId;
    if (!id) return;
    requestAnimationFrame(() => {
      treeEl?.querySelector(`.row[data-id="${id}"]`)?.scrollIntoView({ block: "nearest" });
    });
  });

  function leaveFilter() {
    filterInput?.blur();
    app.focus = "tree";
  }

  function filterKeys(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      app.treeFilter = "";
      app.filterActive = false;
      leaveFilter();
    } else if (e.key === "Enter" || e.key === "ArrowDown") {
      e.preventDefault();
      leaveFilter();
      if (e.key === "ArrowDown") act.selectOffset(1);
      else if (!app.selectedId && rows.length) act.setPageForView(rows[0].node.id);
    }
  }

  function renameKeys(e: InputKeyEvent, id: string) {
    if (e.key === "Enter") {
      e.preventDefault();
      void act.commitRename(id, e.currentTarget.value);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      app.renamingId = null;
      app.focus = "tree";
    }
  }

  function rowClick(e: MouseEvent, id: string) {
    if ((e.target as HTMLElement).closest("input, button")) return;
    act.setPageForView(id);
    app.focus = "tree";
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  class="tree"
  role="tree"
  bind:this={treeEl}
  onpointermove={dndMove}
  onpointerup={dndUp}
  onpointercancel={dndCancel}
  oncontextmenu={openTreeMenu}
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
    {@const node = row.node}
    {@const dropZone = app.dropTarget?.id === node.id ? app.dropTarget.zone : null}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="row"
      class:selected={node.id === app.selectedId}
      class:current={node.id === app.currentPageId}
      class:dragging={node.id === app.dragId}
      class:drop-before={dropZone === "before"}
      class:drop-after={dropZone === "after"}
      class:drop-inside={dropZone === "inside"}
      data-id={node.id}
      role="treeitem"
      aria-selected={node.id === app.selectedId}
      onpointerdown={(e) => dndDown(e, node.id)}
      onclick={(e) => rowClick(e, node.id)}
      ondblclick={() => act.startRename(node.id)}
    >
      {#each Array(row.depth) as _}
        <span class="guide"></span>
      {/each}
      <button
        class="chev"
        class:hidden-chev={node.children.length === 0}
        tabindex="-1"
        onclick={(e) => {
          e.stopPropagation();
          void act.toggleExpand(node.id);
        }}
      >
        {node.expanded ? "▾" : "▸"}
      </button>
      {#if app.renamingId === node.id}
        <input
          class="rename"
          data-esc-local
          value={node.title}
          use:autofocusSelect
          onkeydown={(e) => renameKeys(e, node.id)}
          onblur={(e) => void act.commitRename(node.id, e.currentTarget.value)}
        />
      {:else}
        <span class="title">{node.title}</span>
      {/if}
    </div>
  {/each}
  {#if rows.length === 0}
    <div class="tree-empty">
      {app.treeFilter ? "No matches" : `No pages — ${labelOf("page.new")} to create one`}
    </div>
  {/if}
</div>
