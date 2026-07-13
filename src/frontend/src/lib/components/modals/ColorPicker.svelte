<script lang="ts">
  import * as act from "../../actions";
  import { autofocus } from "../../autofocus";
  import { editorCtl } from "../../editorCtl";
  import { COLOR_PALETTE } from "../../markdown";

  const NAMES = Object.keys(COLOR_PALETTE);
  const COLS = 4;
  const CUSTOM = NAMES.length;
  const RAINBOW = `linear-gradient(90deg, ${Object.values(COLOR_PALETTE).join(",")})`;

  let sel = $state(0);
  let customInput: HTMLInputElement | undefined = $state();

  function wrapSelection(after: string) {
    act.closeModal();
    editorCtl.current?.insert("[", after);
  }

  function keys(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowRight": sel = Math.min(CUSTOM, sel + 1); break;
      case "ArrowLeft": sel = Math.max(0, sel - 1); break;
      case "ArrowDown": sel = Math.min(CUSTOM, sel + COLS); break;
      case "ArrowUp": sel = Math.max(0, sel - COLS); break;
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
        <span class="chip" style:background={RAINBOW}></span>
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
