<script lang="ts">
  import { autofocusSelect } from "../../autofocus";
  import { editorRows, historyRows, rowsFor, type HelpRow } from "../../keys/shortcuts";
  import { app } from "../../state/app.svelte";

  let filter = $state("");

  const query = $derived(filter.trim().toLowerCase());
  const inHistory = $derived(app.helpContext === "history");

  const focusedPane = $derived.by((): { name: string; rows: HelpRow[] } => {
    switch (app.focus) {
      case "tree":
        return { name: "Tree (focused pane)", rows: rowsFor("tree") };
      case "results":
      case "search":
        return { name: "Results (focused pane)", rows: rowsFor("results") };
      default:
        return { name: "Editor (focused pane)", rows: editorRows() };
    }
  });

  function visible(rows: HelpRow[]): HelpRow[] {
    return rows.filter(
      ({ keys, desc, search, gate }) =>
        (!gate || (gate === "git" && app.git?.available)) &&
        (!query || `${keys} ${desc} ${search ?? ""}`.toLowerCase().includes(query)),
    );
  }

  const groups = $derived(
    (inHistory
      ? [{ name: "History", rows: historyRows() }]
      : [
          focusedPane,
          { name: "Panes", rows: rowsFor("pane") },
          { name: "Global", rows: [...rowsFor("global"), ...rowsFor("system")] },
        ]
    ).map(({ name, rows }) => ({ name, rows: visible(rows) })),
  );
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
      {#each groups as group (group.name)}
        <div class="help-group">
          <h3>{group.name}</h3>
          {#each group.rows as { id, keys, desc }}
            <div class="help-row" data-command={id}><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
      {/each}
    </div>
    <div class="hint">{inHistory ? "Esc returns to history" : "Esc closes"} · rebind in Settings</div>
  </div>
</div>
