<script lang="ts">
  import * as act from "../actions";
  import { app } from "../state/app.svelte";
  import { autofocusSelect } from "../autofocus";
  import { MOD_LABEL, SHIFT_LABEL } from "../keys/platform";

  const section = $derived(act.currentSection());
  const editing = $derived(app.creatingSection || app.renamingSection);
  const position = $derived(
    app.notebook ? `${app.sectionIdx + 1}/${app.notebook.sections.length}` : "",
  );

  function commit(value: string) {
    if (app.creatingSection) {
      void act.createSectionNamed(value);
      return;
    }
    if (!app.renamingSection || !section) return;
    app.renamingSection = false;
    void act.renameSection(section.id, value.trim() || "Untitled Section");
  }

  function renameKeys(e: KeyboardEvent) {
    if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
    if (e.key === "Escape") {
      e.stopPropagation();
      if (app.creatingSection) act.cancelNewSection();
      else {
        app.renamingSection = false;
        app.focus = "tree";
      }
    }
  }
</script>

<div class="section-strip">
  <button class="nav" title="Previous section ({MOD_LABEL}+PgUp)" onclick={() => act.gotoSectionOffset(-1)}>◀</button>
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
      title="Go to section ({MOD_LABEL}+G) — double-click to rename"
      onclick={() => act.openSectionPicker("goto")}
      ondblclick={() => (app.renamingSection = true)}
    >
      {section?.name ?? "—"} <span style="opacity:.5">{position}</span>
    </button>
  {/if}
  <button class="nav" title="Next section ({MOD_LABEL}+PgDn)" onclick={() => act.gotoSectionOffset(1)}>▶</button>
  <button class="nav" title="New section ({MOD_LABEL}+{SHIFT_LABEL}+N)" onclick={() => act.newSection()}>＋</button>
</div>
