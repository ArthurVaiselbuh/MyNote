<script lang="ts">
  import { autofocusSelect } from "../../autofocus";
  import { editorRows, historyRows, rowsFor, type HelpRow } from "../../keys/shortcuts";
  import { app } from "../../state/app.svelte";

  let filter = $state("");

  const inHistory = $derived(app.helpContext === "history");

  const paneGroup = $derived.by((): { name: string; rows: HelpRow[] } => {
    switch (app.focus) {
      case "tree":
        return { name: "Tree", rows: rowsFor("tree") };
      case "results":
      case "search":
        return { name: "Results", rows: rowsFor("results") };
      default:
        return { name: "Editor", rows: editorRows() };
    }
  });

  function filtered(rows: HelpRow[]): HelpRow[] {
    const f = filter.trim().toLowerCase();
    return rows
      .filter((r) => !r.gate || (r.gate === "git" && app.git?.available))
      .filter(
        ({ keys, desc, search }) => !f || (keys + " " + desc + " " + (search ?? "")).toLowerCase().includes(f),
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
      {#if inHistory}
        <div class="help-group">
          <h3>History</h3>
          {#each filtered(historyRows()) as { keys, desc }}
            <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
      {:else}
        <div class="help-group">
          <h3>{paneGroup.name} (focused pane)</h3>
          {#each filtered(paneGroup.rows) as { keys, desc }}
            <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
        <div class="help-group">
          <h3>Panes</h3>
          {#each filtered(rowsFor("pane")) as { keys, desc }}
            <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
        <div class="help-group">
          <h3>Global</h3>
          {#each filtered([...rowsFor("global"), ...rowsFor("system")]) as { keys, desc }}
            <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
      {/if}
    </div>
    <div class="hint">{inHistory ? "Esc returns to history" : "Esc closes"} · rebind in Settings</div>
  </div>
</div>
