<script lang="ts">
  import * as act from "../actions";
  import { app } from "../state/app.svelte";
  import { autofocusSelect } from "../autofocus";
  import { hintOf } from "../keys/bindings";
  import { openSectionMenu } from "../menus";

  const section = $derived(act.currentSection());
  const editing = $derived(app.creatingSection || app.renamingSection);
  const position = $derived(
    app.notebook ? `${app.sectionIdx + 1}/${app.notebook.sections.length}` : "",
  );

  function commit(value: string) {
    if (app.creatingSection) void act.createSectionNamed(value);
    else if (app.renamingSection && section) {
      app.renamingSection = false;
      void act.renameSection(section.id, value.trim() || "Untitled Section");
    }
  }

  function cancel() {
    if (app.creatingSection) act.cancelNewSection();
    else {
      app.renamingSection = false;
      app.focus = "tree";
    }
  }

  function renameKeys(e: KeyboardEvent & { currentTarget: HTMLInputElement }) {
    if (e.key === "Enter") e.currentTarget.blur();
    else if (e.key === "Escape") {
      e.stopPropagation();
      cancel();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="section-strip" oncontextmenu={openSectionMenu}>
  <button class="nav" title="Previous section{hintOf('section.prev')}" onclick={() => act.gotoSectionOffset(-1)}>◀</button>
  {#if editing}
    <input
      data-esc-local
      value={app.creatingSection ? "" : (section?.name ?? "")}
      placeholder="New Section"
      use:autofocusSelect
      onkeydown={renameKeys}
      onblur={(e) => commit(e.currentTarget.value)}
    />
  {:else}
    <button
      class="name"
      title="Go to section{hintOf('section.goto')} — double-click to rename"
      onclick={() => act.openSectionPicker("goto")}
      ondblclick={() => (app.renamingSection = true)}
    >
      {section?.name ?? "—"} <span style="opacity:.5">{position}</span>
    </button>
  {/if}
  <button class="nav" title="Next section{hintOf('section.next')}" onclick={() => act.gotoSectionOffset(1)}>▶</button>
  <button class="nav" title="New section{hintOf('section.new')}" onclick={() => act.newSection()}>＋</button>
</div>
