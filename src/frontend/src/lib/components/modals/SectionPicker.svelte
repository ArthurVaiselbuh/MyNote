<script lang="ts">
  import * as act from "../../actions";
  import { autofocusSelect } from "../../autofocus";
  import { app } from "../../state/app.svelte";
  import { countPages } from "../../treeUtils";

  let filter = $state("");
  let sel = $state(0);

  const items = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return (app.notebook?.sections ?? [])
      .map((section, idx) => ({ section, idx }))
      .filter(({ section }) => !f || section.name.toLowerCase().includes(f));
  });

  $effect(() => {
    if (sel >= items.length) sel = Math.max(0, items.length - 1);
  });

  const moving = $derived(app.sectionPickerMode === "move");

  function choose(idx: number) {
    act.closeModal();
    if (moving) {
      const target = app.notebook?.sections[idx];
      if (target) void act.moveSelectedToSection(target);
    } else {
      act.gotoSection(idx);
    }
    app.focus = "tree";
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
      if (items[sel]) choose(items[sel].idx);
    } else if (e.key === "Delete" && e.shiftKey) {
      e.preventDefault();
      if (items[sel]) {
        const id = items[sel].section.id;
        act.closeModal();
        act.deleteSectionWithConfirm(id);
      }
    }
  }

</script>

<div class="modal-backdrop">
  <div class="modal" style:width="440px" role="dialog">
    <div class="modal-title">{moving ? "Move page to section" : "Go to section"}</div>
    <input
      placeholder="filter sections…"
      style="width:100%"
      bind:value={filter}
      use:autofocusSelect
      onkeydown={keys}
    />
    <div class="picker-list">
      {#each items as item, i (item.section.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          class="picker-item"
          class:selected={i === sel}
          onclick={() => choose(item.idx)}
          onmousemove={() => (sel = i)}
        >
          <span>{item.section.name}</span>
          <span class="sub">{countPages(item.section)} pages</span>
        </div>
      {/each}
    </div>
    <div class="hint">↑↓ select · Enter {moving ? "move here" : "open"} · Shift+Del delete section · Esc close</div>
  </div>
</div>
