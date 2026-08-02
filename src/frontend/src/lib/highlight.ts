// Shared by the preview's find panel and the search-results peek: both mark
// matches inside already-rendered markdown, so neither can work from source
// offsets — the DOM is the only place the two agree on.

export function clearHighlights(container: HTMLElement) {
  const marks = container.querySelectorAll("mark.find-hit");
  for (const m of marks) m.replaceWith(...Array.from(m.childNodes));
  container.normalize();
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
    let hit = false;
    const frag = document.createDocumentFragment();
    while ((m = re.exec(text))) {
      if (m[0].length === 0) {
        re.lastIndex++;
        continue;
      }
      hit = true;
      if (m.index > last) frag.append(text.slice(last, m.index));
      const mark = document.createElement("mark");
      mark.className = "find-hit";
      mark.textContent = m[0];
      frag.append(mark);
      found.push(mark);
      last = m.index + m[0].length;
    }
    if (hit) {
      if (last < text.length) frag.append(text.slice(last));
      node.replaceWith(frag);
    }
  }
  return found;
}
