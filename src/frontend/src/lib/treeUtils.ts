import type { Notebook, PageNode, Section } from "./api";

export interface FlatRow {
  node: PageNode;
  depth: number;
}

export interface Located {
  node: PageNode;
  parentId: string | null;
  siblings: PageNode[];
  index: number;
}

export function locate(
  list: PageNode[],
  id: string,
  parentId: string | null = null,
): Located | null {
  for (let i = 0; i < list.length; i++) {
    const node = list[i];
    if (node.id === id) return { node, parentId, siblings: list, index: i };
    const found = locate(node.children, id, node.id);
    if (found) return found;
  }
  return null;
}

export function findNode(list: PageNode[], id: string): PageNode | null {
  return locate(list, id)?.node ?? null;
}

export function hasDescendant(node: PageNode, id: string): boolean {
  return findNode(node.children, id) !== null;
}

export function ancestorsOf(list: PageNode[], id: string, trail: PageNode[] = []): PageNode[] {
  for (const node of list) {
    if (node.id === id) return trail;
    const found = ancestorsOf(node.children, id, [...trail, node]);
    if (found.length > 0) return found;
  }
  return [];
}

export function flatten(pages: PageNode[], filter: string): FlatRow[] {
  const rows: FlatRow[] = [];
  const needle = filter.trim().toLowerCase();
  if (needle) appendMatches(pages, needle, 0, rows);
  else appendExpanded(pages, 0, rows);
  return rows;
}

function appendExpanded(list: PageNode[], depth: number, rows: FlatRow[]) {
  for (const node of list) {
    rows.push({ node, depth });
    if (node.expanded) appendExpanded(node.children, depth + 1, rows);
  }
}

// A page survives the filter when it or any descendant matches, which is only
// known once its subtree has been walked — hence append first, drop after.
function appendMatches(
  list: PageNode[],
  needle: string,
  depth: number,
  rows: FlatRow[],
): boolean {
  let anyMatched = false;
  for (const node of list) {
    const rowStart = rows.length;
    rows.push({ node, depth });
    const keep =
      appendMatches(node.children, needle, depth + 1, rows) ||
      node.title.toLowerCase().includes(needle);
    if (keep) anyMatched = true;
    else rows.length = rowStart;
  }
  return anyMatched;
}

export function sectionOfPage(notebook: Notebook, pageId: string): Section | null {
  for (const section of notebook.sections) {
    if (findNode(section.pages, pageId)) return section;
  }
  return null;
}

export function countSubtree<T extends { children: T[] }>(node: T): number {
  return node.children.reduce((sum, c) => sum + countSubtree(c), 1);
}

export function countPages<T extends { children: T[] }>(section: { pages: T[] }): number {
  return section.pages.reduce((sum, p) => sum + countSubtree(p), 0);
}
