export function focusSelect(node: HTMLInputElement | null | undefined) {
  node?.focus();
  node?.select();
}

export function autofocusSelect(node: HTMLInputElement) {
  requestAnimationFrame(() => focusSelect(node));
}

export function autofocus(node: HTMLElement) {
  requestAnimationFrame(() => node.focus());
}
