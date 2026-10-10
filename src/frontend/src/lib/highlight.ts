// Shared by the preview's find panel and the search-results peek: both mark
// matches inside already-rendered markdown, so neither can work from source
// offsets — the DOM is the only place the two agree on.

export function clearHighlights(container: HTMLElement) {
  const marks = container.querySelectorAll("mark.find-hit");
  for (const mark of marks) mark.replaceWith(...mark.childNodes);
  container.normalize();
}

/** The find cursor. `.current` is the one match a pane has scrolled to, and only
 * one match ever carries it; the new cursor is returned for the caller to hold. */
export function setCurrentMatch(
  prev: HTMLElement | null,
  next: HTMLElement | null,
): HTMLElement | null {
  prev?.classList.remove("current");
  next?.classList.add("current");
  next?.scrollIntoView({ block: "center" });
  return next;
}

// matches only within a single text node — a term split across inline
// formatting (e.g. bold covering half the word) won't be found, the usual
// limit of a per-text-node highlighter
export function applyHighlights(container: HTMLElement, re: RegExp, lineByLine = false): HTMLElement[] {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  let n: Node | null;
  while ((n = walker.nextNode())) nodes.push(n as Text);

  const found: HTMLElement[] = [];
  for (const node of nodes) {
    const text = node.textContent ?? "";
    let last = 0;
    let frag: DocumentFragment | null = null;
    for (const [start, end] of highlightRanges(text, re, lineByLine)) {
      frag ??= document.createDocumentFragment();
      if (start > last) frag.append(text.slice(last, start));
      const mark = document.createElement("mark");
      mark.className = "find-hit";
      mark.textContent = text.slice(start, end);
      frag.append(mark);
      found.push(mark);
      last = end;
    }
    if (frag) {
      if (last < text.length) frag.append(text.slice(last));
      node.replaceWith(frag);
    }
  }
  return found;
}

function* highlightRanges(text: string, re: RegExp, lineByLine: boolean): Generator<[number, number]> {
  let offset = 0;
  while (offset < text.length) {
    const newline = lineByLine ? text.indexOf("\n", offset) : -1;
    let end = newline < 0 ? text.length : newline;
    if (newline >= 0 && end > offset && text[end - 1] === "\r") end--;
    const segment = text.slice(offset, end);
    re.lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = re.exec(segment))) {
      if (match[0].length === 0) {
        re.lastIndex++;
        continue;
      }
      const start = offset + match.index;
      yield [start, start + match[0].length];
    }
    if (newline < 0) break;
    offset = newline + 1;
  }
}
