<script lang="ts">
  import { onDestroy } from "svelte";
  import { autofocusSelect } from "../../autofocus";
  import * as act from "../../actions";
  import {
    COMMANDS,
    CONTEXT_LABELS,
    FIXED_BINDINGS,
    assignChord,
    chordOf,
    chordsOf,
    conflictOf,
    formatChord,
    isDefault,
    resetAllToDefaults,
    resetToDefault,
    unassign,
    type BindingContext,
    type Command,
  } from "../../keys/bindings";
  import { app } from "../../state/app.svelte";

  let filter = $state("");
  let capturing = $state<string | null>(null);
  let captureNote = $state("");

  const CONTEXT_ORDER: BindingContext[] = ["global", "pane", "tree", "results", "history"];

  function visible(command: Command): boolean {
    if (command.gate === "git" && !app.git?.available) return false;
    const f = filter.trim().toLowerCase();
    if (!f) return true;
    const chords = chordsOf(command.id).map(formatChord).join(" ");
    return `${command.desc} ${chords} ${command.search ?? ""}`.toLowerCase().includes(f);
  }

  const groups = $derived(
    CONTEXT_ORDER.map((ctx) => ({
      ctx,
      label: CONTEXT_LABELS[ctx],
      commands: COMMANDS.filter((c) => c.ctx === ctx && visible(c)),
      fixed: FIXED_BINDINGS.filter((b) => b.ctx === ctx),
    })).filter((g) => g.commands.length),
  );

  function startCapture(id: string) {
    capturing = id;
    captureNote = "";
    app.capturingChord = true;
  }

  function stopCapture() {
    capturing = null;
    captureNote = "";
    app.capturingChord = false;
  }

  function capture(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      stopCapture();
      return;
    }
    if (e.key === "Tab") {
      // Tab drives the focus ladder everywhere and can't be reassigned —
      // capturing it here would let a global command swallow it (see bindings.ts)
      captureNote = "Tab can't be reassigned — it drives focus everywhere.";
      return;
    }
    const chord = chordOf(e);
    if (!chord) return; // a modifier on its own — wait for the real key
    if (chord.startsWith("Other+")) {
      captureNote = "That modifier isn't usable in a shortcut.";
      return;
    }
    const stolen = conflictOf(capturing, chord);
    const id = capturing;
    assignChord(id, chord);
    stopCapture();
    if (stolen) app.status = `${formatChord(chord)} taken from “${stolen.desc}”`;
    void act.persistSettings();
  }

  function clear(id: string) {
    unassign(id);
    void act.persistSettings();
  }

  function restore(id: string) {
    resetToDefault(id);
    void act.persistSettings();
  }

  // a stale capture flag would swallow every key in the app
  onDestroy(() => (app.capturingChord = false));

  function restoreAll() {
    resetAllToDefaults();
    stopCapture();
    void act.persistSettings();
  }
</script>

<svelte:window onkeydowncapture={capture} />

<div class="modal-backdrop">
  <div class="modal" style:width="640px" role="dialog">
    <div class="modal-title">Keyboard shortcuts</div>
    <input placeholder="filter shortcuts…" style="width:100%" bind:value={filter} use:autofocusSelect />

    <div class="keybind-list">
      {#each groups as group (group.ctx)}
        <div class="settings-section-label">{group.label}</div>
        {#each group.commands as command (command.id)}
          {@const chords = chordsOf(command.id)}
          <div class="keybind-row" class:capturing={capturing === command.id}>
            <span class="keybind-desc">{command.desc}</span>
            <span class="keybind-chords">
              {#if capturing === command.id}
                <span class="keybind-capture">press a key… (Esc cancels)</span>
              {:else if chords.length}
                {#each chords as chord (chord)}<kbd>{formatChord(chord)}</kbd>{/each}
              {:else}
                <span class="keybind-unassigned">unassigned</span>
              {/if}
            </span>
            <span class="keybind-actions">
              <button onclick={() => startCapture(command.id)}>Change…</button>
              <button disabled={!chords.length} onclick={() => clear(command.id)}>Clear</button>
              <button disabled={isDefault(command.id)} onclick={() => restore(command.id)}>Reset</button>
            </span>
          </div>
        {/each}
        {#if !filter.trim()}
          {#each group.fixed as fixed (fixed.desc + fixed.keys)}
            <div class="keybind-row fixed">
              <span class="keybind-desc">{fixed.desc}</span>
              <span class="keybind-chords"><kbd>{fixed.keys}</kbd></span>
              <span class="keybind-actions"><span class="keybind-fixed-note">fixed</span></span>
            </div>
          {/each}
        {/if}
      {:else}
        <div class="settings-path">No shortcuts match “{filter}”.</div>
      {/each}
    </div>

    {#if captureNote}
      <div class="keybind-note">{captureNote}</div>
    {/if}
    <div class="hint">
      Assigning a chord that is already taken moves it off the other command. Esc and Tab drive the
      focus ladder and every modal, so they stay fixed.
    </div>

    <div class="modal-buttons">
      <button class="keybind-reset-all" onclick={restoreAll}>Reset all to defaults</button>
      <button class="primary" onclick={() => act.escapeModal()}>Done</button>
    </div>
  </div>
</div>
