import { ensureSyntaxTree } from "@codemirror/language";
import type { EditorState, TransactionSpec } from "@codemirror/state";

export function insertLinkTransaction(state: EditorState): TransactionSpec {
  const { from, to } = state.selection.main;
  const tree = ensureSyntaxTree(state, to);
  let node = tree?.resolveInner(from, 1);
  while (node) {
    if (node.name === "Link" && node.to >= to) {
      const destination = node.getChild("URL") ?? node.getChild("LinkLabel");
      if (destination) {
        const delimited = destination.name === "LinkLabel"
          || state.sliceDoc(destination.from, destination.from + 1) === "<";
        return {
          selection: {
            anchor: destination.from + Number(delimited),
            head: destination.to - Number(delimited),
          },
        };
      }
      const destinationStart = node.getChildren("LinkMark")
        .find((mark) => state.sliceDoc(mark.from, mark.to) === "(");
      if (destinationStart) return { selection: { anchor: destinationStart.to } };
    }
    node = node.parent ?? undefined;
  }

  const selected = state.sliceDoc(from, to);
  const selectedUrl = /^https?:\/\/\S+$/i.test(selected);
  const label = selectedUrl ? selected.replace(/[\\`*_[\]<>!]/g, "\\$&") : selected;
  const destination = selectedUrl
    ? (/[()<>\\]/.test(selected)
      ? `<${selected.replace(/[<>\\]/g, (character) => encodeURIComponent(character))}>`
      : selected)
    : "url";
  const text = `[${label}](${destination})`;
  const anchor = selectedUrl || !selected ? from + 1 : from + label.length + 3;
  const head = selectedUrl ? anchor + label.length : selected ? anchor + destination.length : anchor;
  return {
    changes: { from, to, insert: text },
    selection: { anchor, head },
  };
}
