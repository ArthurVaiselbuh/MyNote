import { describe, expect, it } from "vitest";
import type { PageNode } from "./api";
import { ancestorsOf } from "./treeUtils";

function node(id: string, children: PageNode[] = []): PageNode {
  return { id, title: id, expanded: false, children };
}

describe("ancestorsOf", () => {
  it("returns root-to-parent order for a deeply nested page", () => {
    const tree = [node("a", [node("b", [node("c", [node("d")])])])];
    expect(ancestorsOf(tree, "d").map((n) => n.id)).toEqual(["a", "b", "c"]);
  });

  it("returns an empty list for a top-level page", () => {
    const tree = [node("a"), node("b")];
    expect(ancestorsOf(tree, "b")).toEqual([]);
  });

  it("returns an empty list when the id is not found", () => {
    const tree = [node("a", [node("b")])];
    expect(ancestorsOf(tree, "missing")).toEqual([]);
  });

  it("only walks the branch that contains the page", () => {
    const tree = [node("a", [node("target")]), node("x", [node("y", [node("z")])])];
    expect(ancestorsOf(tree, "z").map((n) => n.id)).toEqual(["x", "y"]);
  });
});
