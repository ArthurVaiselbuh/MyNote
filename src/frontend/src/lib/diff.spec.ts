import { describe, expect, it } from "vitest";
import { LCS_CELL_CAP, diffText, type Span } from "./diff";

function textOf(spans: Span[] | null | undefined): string | null {
  return spans ? spans.map((s) => s.text).join("") : null;
}

function marksOf(spans: Span[] | null | undefined): [string, boolean][] | undefined {
  return spans?.map((s): [string, boolean] => [s.text, s.hl]);
}

function numberedLines(count: number, prefix: string): string[] {
  return Array.from({ length: count }, (_, i) => `${prefix}${i}`);
}

function expectStrictlyIncreasing(values: number[]): void {
  for (let i = 1; i < values.length; i++) {
    expect(values[i]).toBeGreaterThan(values[i - 1]);
  }
}

describe("diffText — identical / trivial input", () => {
  it("marks identical text as all same, with no anchors or stats", () => {
    const res = diffText("a\nb\nc", "a\nb\nc");
    expect(res.rows.every((r) => r.kind === "same")).toBe(true);
    expect(res.stats).toEqual({ added: 0, removed: 0, changed: 0 });
    expect(res.anchors).toEqual([]);
    expect(res.degraded).toBe(false);
  });

  it("both empty is zero rows", () => {
    const res = diffText("", "");
    expect(res.rows).toEqual([]);
    expect(res.unified).toEqual([]);
  });

  it("empty left is all additions, covering every line", () => {
    const res = diffText("", "x\ny\nz");
    expect(res.rows.every((r) => r.kind === "add")).toBe(true);
    expect(res.rows.map((r) => textOf(r.right))).toEqual(["x", "y", "z"]);
    expect(res.stats.added).toBe(3);
  });
});

describe("diffText — whole-file coverage (never folded)", () => {
  it("pure insertion at the end keeps every original line present", () => {
    const res = diffText("a\nb\nc", "a\nb\nc\nd\ne");
    const leftLines = res.rows.filter((r) => r.leftNo !== null).map((r) => textOf(r.left));
    expect(leftLines).toEqual(["a", "b", "c"]);
    const rightLines = res.rows.map((r) => textOf(r.right)).filter((text) => text !== null);
    expect(rightLines).toEqual(["a", "b", "c", "d", "e"]);
  });

  it("pure deletion at the start keeps every remaining line present", () => {
    const res = diffText("x\ny\na\nb", "a\nb");
    const rightLines = res.rows.filter((r) => r.rightNo !== null).map((r) => textOf(r.right));
    expect(rightLines).toEqual(["a", "b"]);
    expect(res.stats.removed).toBe(2);
  });

  it("a change deep inside a large file reports correct line numbers on both sides", () => {
    const base = numberedLines(1000, "line ");
    const left = base.join("\n");
    const modified = [...base];
    modified[500] = "CHANGED";
    const right = modified.join("\n");

    const res = diffText(left, right);
    // every line of both sides must appear — nothing collapsed
    expect(res.rows.length).toBe(1000);
    const changedRow = res.rows.find((r) => r.kind === "change");
    expect(changedRow?.leftNo).toBe(501);
    expect(changedRow?.rightNo).toBe(501);
    // lines immediately around the edit are unaffected and correctly numbered
    expect(res.rows[498]).toMatchObject({ kind: "same", leftNo: 499, rightNo: 499 });
    expect(res.rows[501]).toMatchObject({ kind: "same", leftNo: 502, rightNo: 502 });
  });
});

describe("diffText — word-level highlighting", () => {
  it("a single-word edit pairs into one change row with only that word marked", () => {
    const res = diffText("the quick fox", "the slow fox");
    expect(res.stats.changed).toBe(1);
    const row = res.rows.find((r) => r.kind === "change")!;
    expect(marksOf(row.left)).toEqual([
      ["the ", false],
      ["quick", true],
      [" fox", false],
    ]);
    expect(marksOf(row.right)).toEqual([
      ["the ", false],
      ["slow", true],
      [" fox", false],
    ]);
  });

  it("an unrelated line swap does not produce word-level noise", () => {
    const res = diffText("hello world", "goodbye moon entirely different");
    const row = res.rows.find((r) => r.kind === "change")!;
    expect(row.left).toEqual([{ text: "hello world", hl: true }]);
    expect(row.right).toEqual([{ text: "goodbye moon entirely different", hl: true }]);
  });

  it("a multi-line replace pairs lines index-wise", () => {
    const res = diffText("a\nold1\nold2\nz", "a\nnew1\nnew2\nnew3\nz");
    const changes = res.rows.filter((r) => r.kind === "change");
    expect(changes.map((r) => r.leftNo)).toEqual([2, 3]);
    expect(changes.map((r) => r.rightNo)).toEqual([2, 3]);
    const extraAdd = res.rows.find((r) => r.kind === "add");
    expect(extraAdd?.rightNo).toBe(4);
  });
});

describe("diffText — trailing newline / CRLF", () => {
  it("a trailing newline difference alone does not mark lines as changed", () => {
    const res = diffText("a\n", "a");
    expect(res.rows.every((r) => r.kind === "same")).toBe(true);
    expect(res.trailingNewlineChanged).toBe(true);
  });

  it("no trailing newline difference when both agree", () => {
    expect(diffText("a\n", "a\n").trailingNewlineChanged).toBe(false);
    expect(diffText("a", "a").trailingNewlineChanged).toBe(false);
  });

  it("CRLF vs LF with identical content is reported as same lines, flagged separately", () => {
    const res = diffText("a\r\nb\r\n", "a\nb\n");
    expect(res.rows.every((r) => r.kind === "same")).toBe(true);
    expect(res.lineEndingsDiffer).toBe(true);
  });

  it("matching CRLF on both sides does not flag line endings", () => {
    const res = diffText("a\r\nb\r\n", "a\r\nb\r\n");
    expect(res.lineEndingsDiffer).toBe(false);
  });
});

describe("diffText — degraded fallback", () => {
  it("still returns every line of both sides when the LCS cap is exceeded", () => {
    const sideLength = Math.ceil(Math.sqrt(LCS_CELL_CAP)) + 1;
    const left = numberedLines(sideLength, "left-only-").join("\n");
    const right = numberedLines(sideLength, "right-only-").join("\n");
    const res = diffText(left, right);
    expect(res.degraded).toBe(true);
    expect(res.rows.filter((r) => r.leftNo !== null).length).toBe(sideLength);
    expect(res.rows.filter((r) => r.rightNo !== null).length).toBe(sideLength);
  });
});

describe("diffText — unified rows", () => {
  it("every line appears exactly once, deletions before additions within a run", () => {
    const res = diffText("a\nold\nz", "a\nnew\nz");
    const kinds = res.unified.map((r) => r.kind);
    expect(kinds).toEqual(["same", "del", "add", "same"]);
  });

  it("anchors point at run starts and are strictly increasing", () => {
    const res = diffText("a\nb\nc\nd\ne\nf", "a\nX\nc\nd\nY\nf");
    expect(res.anchors.length).toBe(2);
    expectStrictlyIncreasing(res.anchors);
    expect(res.unifiedAnchors.length).toBe(2);
    expectStrictlyIncreasing(res.unifiedAnchors);
  });
});
