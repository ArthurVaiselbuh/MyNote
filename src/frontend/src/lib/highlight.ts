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
export function applyHighlights(container: HTMLElement, re: RegExp): HTMLElement[] {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  let n: Node | null;
  while ((n = walker.nextNode())) nodes.push(n as Text);

  const found: HTMLElement[] = [];
  for (const node of nodes) {
    const text = node.textContent ?? "";
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    let last = 0;
    let frag: DocumentFragment | null = null;
    while ((m = re.exec(text))) {
      if (m[0].length === 0) {
        re.lastIndex++;
        continue;
      }
      frag ??= document.createDocumentFragment();
      if (m.index > last) frag.append(text.slice(last, m.index));
      const mark = document.createElement("mark");
      mark.className = "find-hit";
      mark.textContent = m[0];
      frag.append(mark);
      found.push(mark);
      last = m.index + m[0].length;
    }
    if (frag) {
      if (last < text.length) frag.append(text.slice(last));
      node.replaceWith(frag);
    }
  }
  return found;
}
