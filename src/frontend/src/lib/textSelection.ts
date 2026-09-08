import { isTextEntry } from "./textEntry";

export function clearRenderedSelection() {
  const selection = window.getSelection();
  const anchor = selection?.anchorNode;
  const element = anchor instanceof Element ? anchor : anchor?.parentElement;
  if (element && !isTextEntry(element)) selection?.removeAllRanges();
}

export function selectPreviewContent() {
  const preview = document.getElementById("preview-scroll");
  if (!preview) return;
  const range = document.createRange();
  range.selectNodeContents(preview);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}
