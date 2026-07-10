<script lang="ts">
  import * as act from "../actions";
  import { app } from "../state/app.svelte";
  import { autofocusSelect } from "../autofocus";

  const section = $derived(act.currentSection());
  const position = $derived(
    app.notebook ? `${app.sectionIdx + 1}/${app.notebook.sections.length}` : "",
  );

  function commit(value: string) {
    if (!app.renamingSection || !section) return;
    app.renamingSection = false;
    void act.renameSection(section.id, value.trim() || "Untitled Section");
  }

  function renameKeys(e: KeyboardEvent) {
    if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
    if (e.key === "Escape") {
      e.stopPropagation();
      app.renamingSection = false;
      app.focus = "tree";
    }
  }
</script>

<div class="section-strip">
  <button class="nav" title="Previous section (Ctrl+PgUp)" onclick={() => act.gotoSectionOffset(-1)}>◀</button>
  {#if app.renamingSection && section}
    <input
      data-esc-local
      value={section.name}
      use:autofocusSelect
      onkeydown={renameKeys}
      onblur={(e) => commit(e.currentTarget.value)}
    />
  {:else}
    <button
      class="name"
      title="Go to section (Ctrl+G) — double-click to rename"
      onclick={() => act.openSectionPicker("goto")}
      ondblclick={() => (app.renamingSection = true)}
    >
      {section?.name ?? "—"} <span style="opacity:.5">{position}</span>
    </button>
  {/if}
  <button class="nav" title="Next section (Ctrl+PgDn)" onclick={() => act.gotoSectionOffset(1)}>▶</button>
  <button class="nav" title="New section (Ctrl+Shift+N)" onclick={() => act.newSection()}>＋</button>
</div>
