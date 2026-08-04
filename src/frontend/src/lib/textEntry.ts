// Text-entry surfaces are the seam two different rules share: the key
// dispatcher suppresses pane-local keys inside them, and the context menu
// leaves them to the webview's native one (right-click paste, spell check).
export function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  if (target.isContentEditable) return true;
  return target.closest(".cm-editor") !== null;
}
