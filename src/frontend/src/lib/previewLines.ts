import { LINE_ATTR } from "./markdown";

// Reading a body line out of the rendered preview and scrolling back to one.
// Only block elements carry LINE_ATTR, so a position is always resolved to the
// block that contains it.

function blocksIn(container: HTMLElement): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(`[${LINE_ATTR}]`)];
}

function bodyLineOf(block: HTMLElement): number {
  return Number(block.getAttribute(LINE_ATTR));
}

function offsetFromContainerTop(container: HTMLElement, block: HTMLElement): number {
  return block.getBoundingClientRect().top - container.getBoundingClientRect().top;
}

// nested blocks come after their parent in document order, so the last block
// still satisfying the test is the innermost one
function lastBlockWhile(
  container: HTMLElement,
  keepGoing: (block: HTMLElement) => boolean,
): HTMLElement | null {
  const blocks = blocksIn(container);
  let last: HTMLElement | null = null;
  for (const block of blocks) {
    if (!keepGoing(block)) break;
    last = block;
  }
  return last ?? blocks[0] ?? null;
}

export function previewPositionAt(
  container: HTMLElement,
  focused: Element | null,
): { bodyLine: number; offsetFromTop: number } | null {
  const block =
    focused?.closest<HTMLElement>(`[${LINE_ATTR}]`) ??
    lastBlockWhile(container, (b) => offsetFromContainerTop(container, b) <= 1);
  if (!block) return null;
  return {
    bodyLine: bodyLineOf(block),
    offsetFromTop: offsetFromContainerTop(container, block),
  };
}

export function scrollPreviewToBodyLine(
  container: HTMLElement,
  bodyLine: number,
  offsetFromTop: number,
) {
  const block = lastBlockWhile(container, (b) => bodyLineOf(b) <= bodyLine);
  if (!block) return;
  container.scrollTop += offsetFromContainerTop(container, block) - offsetFromTop;
}
