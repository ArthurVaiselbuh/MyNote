<script lang="ts">
  import { autofocusSelect } from "../../autofocus";
  import {
    EDITOR_SHORTCUTS,
    GLOBAL_SHORTCUTS,
    HISTORY_SHORTCUTS,
    PANE_SHORTCUTS,
    RESULTS_SHORTCUTS,
    TREE_SHORTCUTS,
    type Shortcut,
  } from "../../keys/shortcuts";
  import { app } from "../../state/app.svelte";

  let filter = $state("");

  const inHistory = $derived(app.helpContext === "history");

  const paneGroup = $derived.by((): { name: string; rows: Shortcut[] } => {
    switch (app.focus) {
      case "tree":
        return { name: "Tree", rows: TREE_SHORTCUTS };
      case "results":
      case "search":
        return { name: "Results", rows: RESULTS_SHORTCUTS };
      default:
        return { name: "Editor", rows: EDITOR_SHORTCUTS };
    }
  });

  function filtered(rows: Shortcut[]): Shortcut[] {
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
          {#each filtered(HISTORY_SHORTCUTS) as { keys, desc }}
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
          {#each filtered(PANE_SHORTCUTS) as { keys, desc }}
            <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
        <div class="help-group">
          <h3>Global</h3>
          {#each filtered(GLOBAL_SHORTCUTS) as { keys, desc }}
            <div class="help-row"><span>{desc}</span><kbd>{keys}</kbd></div>
          {/each}
        </div>
      {/if}
    </div>
    <div class="hint">{inHistory ? "Esc returns to history" : "Esc closes"}</div>
  </div>
</div>
