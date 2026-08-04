import {
  COMMANDS,
  FIXED_BINDINGS,
  labelsOf,
  type BindingContext,
  type Command,
} from "./bindings";

// The rows the help overlay renders. Everything here resolves through
// bindings.ts, so a rebound or unassigned chord shows up in the cheat sheet the
// moment it changes — bindings.ts is the source of truth, this file only groups
// them for reading.
export type HelpRow = { keys: string; desc: string; search?: string; gate?: "git" };

function row(command: Command): HelpRow {
  return {
    keys: labelsOf(command.id) || "—",
    desc: command.desc,
    search: command.search,
    gate: command.gate,
  };
}

function fixedRows(ctx: BindingContext): HelpRow[] {
  return FIXED_BINDINGS.filter((b) => b.ctx === ctx).map(({ keys, desc }) => ({ keys, desc }));
}

export function rowsFor(ctx: BindingContext): HelpRow[] {
  return [...COMMANDS.filter((c) => c.ctx === ctx).map(row), ...fixedRows(ctx)];
}

// The editor pane has no keys of its own — the ones that matter while writing
// are globals, gathered here so the focused-pane group is still useful there.
const EDITOR_ROW_IDS = ["app.find", "app.findNext", "app.findPrev", "app.insertHelper", "app.toggleMode"];

export function editorRows(): HelpRow[] {
  const byId = new Map(COMMANDS.map((c) => [c.id, c]));
  const rows = EDITOR_ROW_IDS.map((id) => byId.get(id)).filter((c): c is Command => !!c).map(row);
  return [...rows, { keys: "Esc", desc: "Close find / focus tree" }];
}
