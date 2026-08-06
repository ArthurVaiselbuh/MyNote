<script lang="ts">
  import * as act from "../../actions";
  import { autofocus } from "../../autofocus";
  import { clampIndex } from "../../listIndex";
  import { COLOR_PALETTE, COLOR_PALETTE_GRADIENT } from "../../markdown";
  import { editorCtl } from "../../paneCtl";

  const NAMES = Object.keys(COLOR_PALETTE);
  const COLS = 4;
  const CUSTOM = NAMES.length;
  const CELL_COUNT = CUSTOM + 1;

  let sel = $state(0);
  let customInput: HTMLInputElement | undefined = $state();

  function wrapSelection(after: string) {
    act.closeModal();
    editorCtl.current?.insert("[", after);
  }

  function keys(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowRight": sel = clampIndex(sel + 1, CELL_COUNT); break;
      case "ArrowLeft": sel = clampIndex(sel - 1, CELL_COUNT); break;
      case "ArrowDown": sel = clampIndex(sel + COLS, CELL_COUNT); break;
      case "ArrowUp": sel = clampIndex(sel - COLS, CELL_COUNT); break;
      case "Enter":
        if (sel === CUSTOM) customInput?.click();
        else wrapSelection(`]{.${NAMES[sel]}}`);
        break;
      case "Tab": break; // keep focus on the grid
      default: return;
    }
    e.preventDefault();
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="360px" role="dialog">
    <div class="modal-title">Text color</div>
    <!-- svelte-ignore a11y_no_static_element_interactions a11y_no_noninteractive_tabindex -->
    <div class="color-grid" tabindex="-1" use:autofocus onkeydown={keys}>
      {#each NAMES as name, i (name)}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          class="color-cell"
          class:selected={i === sel}
          onclick={() => wrapSelection(`]{.${name}}`)}
          onmousemove={() => (sel = i)}
        >
          <span class="chip" style:background={COLOR_PALETTE[name]}></span>
          {name}
        </div>
      {/each}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        class="color-cell custom"
        class:selected={sel === CUSTOM}
        onclick={() => customInput?.click()}
        onmousemove={() => (sel = CUSTOM)}
      >
        <span class="chip" style:background={COLOR_PALETTE_GRADIENT}></span>
        Custom…
        <input
          type="color"
          bind:this={customInput}
          onclick={(e) => e.stopPropagation()}
          onchange={(e) => wrapSelection(`]{style="color:${e.currentTarget.value}"}`)}
        />
      </div>
    </div>
    <div class="hint">↑↓←→ select · Enter insert (wraps selection) · Esc back</div>
  </div>
</div>
