<script lang="ts">
  import * as act from "../../actions";
  import type { RecentNotebook } from "../../api";
  import { autofocus } from "../../autofocus";
  import { app } from "../../state/app.svelte";

  type Item =
    | { kind: "recent"; nb: RecentNotebook }
    | { kind: "new" }
    | { kind: "browse" };

  const items = $derived.by((): Item[] => [
    ...app.recentNotebooks.map((nb): Item => ({ kind: "recent", nb })),
    { kind: "new" },
    { kind: "browse" },
  ]);

  let sel = $state(0);

  $effect(() => {
    if (sel >= items.length) sel = Math.max(0, items.length - 1);
  });

  function choose(item: Item) {
    if (item.kind === "new") {
      void act.createNotebookDialog();
    } else if (item.kind === "browse") {
      void act.browseNotebookFile();
    } else if (!item.nb.exists) {
      app.status = "notebook.json not found — the folder was moved or deleted";
    } else {
      void act.openNotebookAt(item.nb.path);
    }
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      sel = Math.min(items.length - 1, sel + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (items[sel]) choose(items[sel]);
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="modal-backdrop">
  <div class="modal open-nb" style:width="540px" role="dialog" tabindex="-1" use:autofocus>
    <img class="open-nb-logo" src="/logo.png" alt="MyNote logo" />
    <div class="open-nb-name">MyNote</div>

    <div class="open-nb-list">
      <div class="open-nb-heading">Recent notebooks</div>
      {#if app.recentNotebooks.length === 0}
        <div class="open-nb-empty">No recent notebooks yet</div>
      {/if}
      {#each items as item, i (i)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="recent-item"
          class:selected={i === sel}
          class:missing={item.kind === "recent" && !item.nb.exists}
          class:action={item.kind !== "recent"}
          onclick={() => choose(item)}
          onmousemove={() => (sel = i)}
        >
          {#if item.kind === "recent"}
            <div class="nb-name">
              <span>{item.nb.name}</span>
              {#if item.nb.path === app.root}<span class="nb-badge">current</span>{/if}
              {#if !item.nb.exists}<span class="nb-badge">missing</span>{/if}
            </div>
            <div class="nb-path">{item.nb.path}</div>
          {:else if item.kind === "new"}
            <div class="nb-name"><span>New notebook…</span></div>
            <div class="nb-path">create an empty notebook in a folder you choose</div>
          {:else}
            <div class="nb-name"><span>Open notebook…</span></div>
            <div class="nb-path">select the notebook.json inside a notebook folder</div>
          {/if}
        </div>
      {/each}
    </div>

    <div class="hint">↑↓ select · Enter open · Esc close</div>
  </div>
</div>
