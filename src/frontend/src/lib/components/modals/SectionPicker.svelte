<script lang="ts">
  import * as act from "../../actions";
  import { autofocusSelect } from "../../autofocus";
  import { app } from "../../state/app.svelte";
  import { countPages, sectionOfPage } from "../../treeUtils";
  import { ALT_LABEL } from "../../keys/platform";

  const moving = $derived(app.sectionPickerMode === "move");

  function currentSectionIndex(): number {
    if (!app.notebook) return 0;
    if (moving && app.selectedId) {
      const source = sectionOfPage(app.notebook, app.selectedId);
      const idx = source ? app.notebook.sections.indexOf(source) : -1;
      return idx >= 0 ? idx : 0;
    }
    return app.sectionIdx;
  }

  let filter = $state("");
  let sel = $state(currentSectionIndex());
  let listEl = $state<HTMLElement>();

  const items = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return (app.notebook?.sections ?? [])
      .map((section, idx) => ({ section, idx }))
      .filter(({ section }) => !f || section.name.toLowerCase().includes(f));
  });

  $effect(() => {
    if (sel >= items.length) sel = Math.max(0, items.length - 1);
  });

  $effect(() => {
    (listEl?.children[sel] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
  });

  function reorder(dir: number) {
    if (filter.trim()) return;
    const cur = items[sel];
    if (!cur) return;
    const target = cur.idx + dir;
    if (target < 0 || target >= (app.notebook?.sections.length ?? 0)) return;
    void act.moveSection(cur.section.id, target);
    sel = target;
  }

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
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      reorder(e.key === "ArrowDown" ? 1 : -1);
    } else if (e.key === "ArrowDown") {
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
    <div class="picker-list" bind:this={listEl}>
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
    <div class="hint">↑↓ select · {ALT_LABEL}+↑↓ reorder · Enter {moving ? "move here" : "open"} · Shift+Del delete section · Esc close</div>
  </div>
</div>
