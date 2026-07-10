<script lang="ts">
  import { autofocusSelect } from "../../autofocus";
  import { app } from "../../state/app.svelte";

  type Row = [string, string];

  const GLOBAL: Row[] = [
    ["Ctrl+K", "Global search"],
    ["Ctrl+F", "Find in page"],
    ["Ctrl+E", "Edit / Preview"],
    ["Ctrl+S", "Save"],
    ["Ctrl+Z / Y", "Undo / redo delete & move"],
    ["Ctrl+N", "New page"],
    ["Ctrl+Shift+N", "New section"],
    ["Ctrl+1 / 2 / 3", "Focus tree / editor / title"],
    ["Ctrl+J", "Insert helper"],
    ["Ctrl+PgUp / PgDn", "Prev / next section"],
    ["Ctrl+G", "Go to section"],
    ["Ctrl+Shift+G", "Move page to section"],
    ["Ctrl+O", "Open notebook"],
    ["Ctrl+I", "Import OneNote .mht"],
    ["Ctrl+,", "Settings"],
    ["Ctrl+= / − / 0", "Zoom in / out / reset"],
    ["F3 / Shift+F3", "Next / prev match"],
    ["?", "This overlay"],
  ];

  const PANES: Row[] = [
    ["Tab / Shift+Tab", "Cycle panes"],
    ["PgUp / PgDn", "Scroll active view"],
    ["Esc", "Back out one layer"],
  ];

  const TREE: Row[] = [
    ["/", "Filter pages"],
    ["↑ / ↓", "Select page"],
    ["← / →", "Collapse / expand"],
    ["Enter", "Open in editor"],
    ["Ctrl+Enter", "New subpage"],
    ["F2", "Rename"],
    ["Alt+↑ / ↓", "Move up / down"],
    ["Ctrl+] / [", "Demote / promote"],
    ["Del", "Delete"],
    ["Alt+← / →", "Move to prev / next section"],
    ["Ctrl+Shift+G", "Move to section…"],
  ];

  const EDITOR: Row[] = [
    ["Ctrl+F", "Find in page"],
    ["F3 / Shift+F3", "Next / prev match"],
    ["Ctrl+J", "Insert helper"],
    ["Esc", "Close find / focus tree"],
  ];

  const RESULTS: Row[] = [
    ["↑ / ↓", "Select result"],
    ["Enter", "Open result"],
    ["/", "Refine query"],
    ["Esc", "Back to page"],
  ];

  let filter = $state("");

  const paneGroup = $derived.by((): { name: string; rows: Row[] } => {
    switch (app.focus) {
      case "tree":
        return { name: "Tree", rows: TREE };
      case "results":
      case "search":
        return { name: "Results", rows: RESULTS };
      default:
        return { name: "Editor", rows: EDITOR };
    }
  });

  function filtered(rows: Row[]): Row[] {
    const f = filter.trim().toLowerCase();
    if (!f) return rows;
    return rows.filter(([keys, desc]) =>
      (keys + " " + desc).toLowerCase().includes(f),
    );
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="760px" role="dialog">
    <div class="modal-title">Keyboard shortcuts</div>
    <input
      placeholder="filter shortcuts…"
      style="width:100%"
      bind:value={filter}
      use:autofocusSelect
    />
    <div class="help-groups">
      <div class="help-group">
        <h3>{paneGroup.name} (focused pane)</h3>
        {#each filtered(paneGroup.rows) as [keys, desc]}
          <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
        {/each}
      </div>
      <div class="help-group">
        <h3>Panes</h3>
        {#each filtered(PANES) as [keys, desc]}
          <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
        {/each}
      </div>
      <div class="help-group">
        <h3>Global</h3>
        {#each filtered(GLOBAL) as [keys, desc]}
          <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
        {/each}
      </div>
    </div>
    <div class="hint">Esc closes</div>
  </div>
</div>
