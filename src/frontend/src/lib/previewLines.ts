import { LINE_ATTR } from "./markdown";

// Reading a body line out of the rendered preview and scrolling back to one.
// Only block elements carry LINE_ATTR, so a position is always resolved to the
// block that contains it.

function bodyLineOf(block: HTMLElement): number {
  return Number(block.getAttribute(LINE_ATTR));
}

function topOf(el: Element): number {
  return el.getBoundingClientRect().top;
}

function offsetBelow(containerTop: number, block: HTMLElement): number {
  return topOf(block) - containerTop;
}

// nested blocks come after their parent in document order, so the last block
// still satisfying the test is the innermost one
function lastBlockWhile(
  container: HTMLElement,
  keepGoing: (block: HTMLElement) => boolean,
): HTMLElement | null {
  const blocks = container.querySelectorAll<HTMLElement>(`[${LINE_ATTR}]`);
  let last: HTMLElement | null = blocks[0] ?? null;
  for (const block of blocks) {
    if (!keepGoing(block)) break;
    last = block;
  }
  return last;
}

export function previewPositionAt(
  container: HTMLElement,
  focused: Element | null,
): { bodyLine: number; offsetRatio: number } | null {
  const containerTop = topOf(container);
  const block =
    focused?.closest<HTMLElement>(`[${LINE_ATTR}]`) ??
    lastBlockWhile(container, (b) => offsetBelow(containerTop, b) <= 1);
  if (!block) return null;
  const height = block.getBoundingClientRect().height;
  return {
    bodyLine: bodyLineOf(block),
    offsetRatio: height > 0 ? clampRatio(-offsetBelow(containerTop, block) / height) : 0,
  };
}

export function scrollPreviewToBodyLine(
  container: HTMLElement,
  bodyLine: number,
  offsetRatio: number,
) {
  const block = lastBlockWhile(container, (b) => bodyLineOf(b) <= bodyLine);
  if (!block) return;
  const height = block.getBoundingClientRect().height;
  // getBoundingClientRect is viewport-relative, so raising scrollTop moves a
  // block's on-screen top *up* — the delta needed is the block's current
  // offset plus how far into it we want to land, not minus
  container.scrollTop += offsetBelow(topOf(container), block) + offsetRatio * height;
}

function clampRatio(x: number): number {
  return Math.min(1, Math.max(0, x));
}
