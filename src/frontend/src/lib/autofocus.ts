export function autofocusSelect(node: HTMLInputElement) {
  requestAnimationFrame(() => {
    node.focus();
    node.select();
  });
}

export function autofocus(node: HTMLElement) {
  requestAnimationFrame(() => node.focus());
}
