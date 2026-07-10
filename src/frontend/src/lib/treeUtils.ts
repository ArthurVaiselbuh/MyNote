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

export function findNode(list: PageNode[], id: string): PageNode | null {
  for (const node of list) {
    if (node.id === id) return node;
    const found = findNode(node.children, id);
    if (found) return found;
  }
  return null;
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

export function isDescendant(list: PageNode[], ancestorId: string, id: string): boolean {
  const ancestor = findNode(list, ancestorId);
  return ancestor ? findNode(ancestor.children, id) !== null : false;
}

function matchFilter(node: PageNode, filter: string): boolean {
  return (
    node.title.toLowerCase().includes(filter) ||
    node.children.some((c) => matchFilter(c, filter))
  );
}

export function flatten(pages: PageNode[], filter: string): FlatRow[] {
  const out: FlatRow[] = [];
  const f = filter.trim().toLowerCase();
  const walk = (list: PageNode[], depth: number) => {
    for (const node of list) {
      if (f && !matchFilter(node, f)) continue;
      out.push({ node, depth });
      if (f || node.expanded) walk(node.children, depth + 1);
    }
  };
  walk(pages, 0);
  return out;
}

export function sectionOfPage(notebook: Notebook, pageId: string): Section | null {
  for (const section of notebook.sections) {
    if (findNode(section.pages, pageId)) return section;
  }
  return null;
}

export function countSubtree(node: PageNode): number {
  return node.children.reduce((sum, c) => sum + countSubtree(c), 1);
}

export function countPages(section: Section): number {
  return section.pages.reduce((sum, p) => sum + countSubtree(p), 0);
}
