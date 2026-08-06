// Whole-file line + word diff for the History pane. No dependency — git's own
// patch output isn't used because one side is routinely the uncommitted
// on-disk buffer, and both diff modes must render every line of the file
// (never fold unchanged runs), which a hunk-based patch can't give us.

export type RowKind = "same" | "add" | "del" | "change";

export interface Span {
  text: string;
  hl: boolean;
}

export interface AlignedRow {
  kind: RowKind;
  leftNo: number | null;
  left: Span[] | null;
  rightNo: number | null;
  right: Span[] | null;
}

export interface UnifiedRow {
  kind: "same" | "add" | "del";
  leftNo: number | null;
  rightNo: number | null;
  spans: Span[];
}

export interface DiffResult {
  /** Every line of both sides, aligned side by side. Never folded. */
  rows: AlignedRow[];
  /** Every line of both sides, del-before-add within a change. Never folded. */
  unified: UnifiedRow[];
  /** Row index of the first row of each non-"same" run, in `rows`. */
  anchors: number[];
  /** Same, but indexing into `unified`. */
  unifiedAnchors: number[];
  stats: { added: number; removed: number; changed: number };
  /** The middle region was too large for exact LCS — shown as one del/add
   * block instead (still every line, just not minimally matched). */
  degraded: boolean;
  trailingNewlineChanged: boolean;
  /** One side has \r-terminated lines and the other doesn't. */
  lineEndingsDiffer: boolean;
}

export const LCS_CELL_CAP = 4_000_000;
const WORD_SPAN_BUDGET = 800;
const MAX_WORD_TOKENS = 400;
const SIMILARITY_THRESHOLD = 0.3;

function splitLines(text: string): { lines: string[]; hadTrailingNewline: boolean } {
  const lines = text.split("\n");
  const hadTrailingNewline = lines[lines.length - 1] === "";
  if (hadTrailingNewline) lines.pop();
  return { lines, hadTrailingNewline };
}

function endsWithCR(line: string): boolean {
  return line.endsWith("\r");
}

function withoutCR(line: string): string {
  return endsWithCR(line) ? line.slice(0, -1) : line;
}

/** Maps strings to dense ids so the LCS can compare numbers. */
class Interner {
  private readonly ids = new Map<string, number>();

  idOf(text: string): number {
    let id = this.ids.get(text);
    if (id === undefined) {
      id = this.ids.size;
      this.ids.set(text, id);
    }
    return id;
  }
}

function trimCommon(a: number[], b: number[]): { prefix: number; suffix: number } {
  const maxPrefix = Math.min(a.length, b.length);
  let prefix = 0;
  while (prefix < maxPrefix && a[prefix] === b[prefix]) prefix++;
  const maxSuffix = maxPrefix - prefix;
  let suffix = 0;
  while (suffix < maxSuffix && a[a.length - 1 - suffix] === b[b.length - 1 - suffix]) suffix++;
  return { prefix, suffix };
}

type Op = "same" | "del" | "add";

/** Classic LCS diff over two interned-id arrays. Exact when `a.length *
 * b.length` fits `LCS_CELL_CAP`; otherwise the whole region is emitted as
 * one del-block then one add-block (still every element, not minimally
 * matched — `degraded` tells the caller). */
function lcsOps(a: number[], b: number[]): { ops: Op[]; degraded: boolean } {
  const n = a.length;
  const m = b.length;
  if (n * m > LCS_CELL_CAP) {
    return { ops: new Array<Op>(n).fill("del").concat(new Array<Op>(m).fill("add")), degraded: true };
  }
  const width = m + 1;
  const dp = new Int32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    const row = i * width;
    const rowBelow = row + width;
    const ai = a[i];
    for (let j = m - 1; j >= 0; j--) {
      dp[row + j] = ai === b[j] ? dp[rowBelow + j + 1] + 1 : Math.max(dp[rowBelow + j], dp[row + j + 1]);
    }
  }
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      ops.push("same");
      i++;
      j++;
    } else if (dp[(i + 1) * width + j] >= dp[i * width + (j + 1)]) {
      ops.push("del");
      i++;
    } else {
      ops.push("add");
      j++;
    }
  }
  while (i < n) {
    ops.push("del");
    i++;
  }
  while (j < m) {
    ops.push("add");
    j++;
  }
  return { ops, degraded: false };
}

type LineOp = { kind: "same"; ai: number; bi: number } | { kind: "del"; ai: number } | { kind: "add"; bi: number };

function buildLineOps(leftLines: string[], rightLines: string[]): { ops: LineOp[]; degraded: boolean } {
  const interner = new Interner();
  const a = leftLines.map((line) => interner.idOf(withoutCR(line)));
  const b = rightLines.map((line) => interner.idOf(withoutCR(line)));
  const { prefix, suffix } = trimCommon(a, b);

  const ops: LineOp[] = [];
  for (let k = 0; k < prefix; k++) ops.push({ kind: "same", ai: k, bi: k });

  const { ops: midOps, degraded } = lcsOps(a.slice(prefix, a.length - suffix), b.slice(prefix, b.length - suffix));
  let ai = prefix;
  let bi = prefix;
  for (const kind of midOps) {
    if (kind === "same") {
      ops.push({ kind: "same", ai: ai++, bi: bi++ });
    } else if (kind === "del") {
      ops.push({ kind: "del", ai: ai++ });
    } else {
      ops.push({ kind: "add", bi: bi++ });
    }
  }

  const leftSuffixStart = leftLines.length - suffix;
  const rightSuffixStart = rightLines.length - suffix;
  for (let k = 0; k < suffix; k++) {
    ops.push({ kind: "same", ai: leftSuffixStart + k, bi: rightSuffixStart + k });
  }
  return { ops, degraded };
}

const WORD_TOKEN = /[\p{L}\p{N}_]+|\s+|[^\s\p{L}\p{N}_]/gu;

function tokenizeForDiff(line: string): string[] {
  return line.match(WORD_TOKEN) ?? (line.length ? [line] : []);
}

function bagSimilarity(a: string[], b: string[]): number {
  if (a.length === 0 && b.length === 0) return 1;
  const freq = new Map<string, number>();
  for (const t of a) freq.set(t, (freq.get(t) ?? 0) + 1);
  let common = 0;
  for (const t of b) {
    const c = freq.get(t) ?? 0;
    if (c > 0) {
      common++;
      freq.set(t, c - 1);
    }
  }
  return (2 * common) / (a.length + b.length);
}

function spansOf(text: string, hl: boolean): Span[] {
  return [{ text, hl }];
}

function wholeLineChanged(a: string, b: string): { left: Span[]; right: Span[] } {
  return { left: spansOf(a, true), right: spansOf(b, true) };
}

function pushSpan(spans: Span[], text: string, hl: boolean) {
  const last = spans[spans.length - 1];
  if (last && last.hl === hl) {
    last.text += text;
  } else {
    spans.push({ text, hl });
  }
}

/** Word-level spans for one changed line pair, falling back to whole-line
 * highlighting once the page has more changes than are worth tokenizing, when
 * either line is huge, or when the two lines are too unalike to have been an
 * edit of each other. */
function diffWords(a: string, b: string, changedSoFar: number): { left: Span[]; right: Span[] } {
  if (changedSoFar >= WORD_SPAN_BUDGET) return wholeLineChanged(a, b);
  if (a === b) return { left: spansOf(a, false), right: spansOf(b, false) };

  const leftTokens = tokenizeForDiff(a);
  const rightTokens = tokenizeForDiff(b);
  if (leftTokens.length > MAX_WORD_TOKENS || rightTokens.length > MAX_WORD_TOKENS) return wholeLineChanged(a, b);
  if (bagSimilarity(leftTokens, rightTokens) < SIMILARITY_THRESHOLD) return wholeLineChanged(a, b);

  const interner = new Interner();
  const { ops } = lcsOps(
    leftTokens.map((t) => interner.idOf(t)),
    rightTokens.map((t) => interner.idOf(t)),
  );

  const left: Span[] = [];
  const right: Span[] = [];
  let li = 0;
  let ri = 0;
  for (const kind of ops) {
    if (kind === "same") {
      pushSpan(left, leftTokens[li++], false);
      pushSpan(right, rightTokens[ri++], false);
    } else if (kind === "del") {
      pushSpan(left, leftTokens[li++], true);
    } else {
      pushSpan(right, rightTokens[ri++], true);
    }
  }
  return { left, right };
}

/** Collects both renderings at once, so the side-by-side and unified views
 * can never drift apart. Line indices are 0-based; the rows carry them 1-based. */
class DiffRows {
  readonly rows: AlignedRow[] = [];
  readonly unified: UnifiedRow[] = [];
  readonly anchors: number[] = [];
  readonly unifiedAnchors: number[] = [];
  added = 0;
  removed = 0;
  changed = 0;

  startRun() {
    this.anchors.push(this.rows.length);
    this.unifiedAnchors.push(this.unified.length);
  }

  same(ai: number, bi: number, leftText: string, rightText: string) {
    this.rows.push({
      kind: "same",
      leftNo: ai + 1,
      left: spansOf(leftText, false),
      rightNo: bi + 1,
      right: spansOf(rightText, false),
    });
    this.unified.push({ kind: "same", leftNo: ai + 1, rightNo: bi + 1, spans: spansOf(leftText, false) });
  }

  change(ai: number, bi: number, left: Span[], right: Span[]) {
    this.rows.push({ kind: "change", leftNo: ai + 1, left, rightNo: bi + 1, right });
    this.unified.push({ kind: "del", leftNo: ai + 1, rightNo: null, spans: left });
    this.unified.push({ kind: "add", leftNo: null, rightNo: bi + 1, spans: right });
    this.changed++;
  }

  del(ai: number, text: string) {
    this.rows.push({ kind: "del", leftNo: ai + 1, left: spansOf(text, true), rightNo: null, right: null });
    this.unified.push({ kind: "del", leftNo: ai + 1, rightNo: null, spans: spansOf(text, true) });
    this.removed++;
  }

  add(bi: number, text: string) {
    this.rows.push({ kind: "add", leftNo: null, left: null, rightNo: bi + 1, right: spansOf(text, true) });
    this.unified.push({ kind: "add", leftNo: null, rightNo: bi + 1, spans: spansOf(text, true) });
    this.added++;
  }

  finish(flags: Pick<DiffResult, "degraded" | "trailingNewlineChanged" | "lineEndingsDiffer">): DiffResult {
    const { rows, unified, anchors, unifiedAnchors, added, removed, changed } = this;
    return { rows, unified, anchors, unifiedAnchors, stats: { added, removed, changed }, ...flags };
  }
}

export function diffText(leftText: string, rightText: string): DiffResult {
  const { lines: leftLines, hadTrailingNewline: leftTrail } = splitLines(leftText);
  const { lines: rightLines, hadTrailingNewline: rightTrail } = splitLines(rightText);
  const { ops, degraded } = buildLineOps(leftLines, rightLines);

  const out = new DiffRows();
  let i = 0;
  while (i < ops.length) {
    const op = ops[i];
    if (op.kind === "same") {
      out.same(op.ai, op.bi, leftLines[op.ai], rightLines[op.bi]);
      i++;
      continue;
    }

    const dels: number[] = [];
    const adds: number[] = [];
    while (i < ops.length && ops[i].kind !== "same") {
      const o = ops[i++];
      if (o.kind === "del") dels.push(o.ai);
      else if (o.kind === "add") adds.push(o.bi);
    }
    out.startRun();

    const paired = Math.min(dels.length, adds.length);
    for (let k = 0; k < paired; k++) {
      const { left, right } = diffWords(leftLines[dels[k]], rightLines[adds[k]], out.changed);
      out.change(dels[k], adds[k], left, right);
    }
    for (let k = paired; k < dels.length; k++) out.del(dels[k], leftLines[dels[k]]);
    for (let k = paired; k < adds.length; k++) out.add(adds[k], rightLines[adds[k]]);
  }

  return out.finish({
    degraded,
    trailingNewlineChanged: leftTrail !== rightTrail,
    lineEndingsDiffer: leftLines.some(endsWithCR) !== rightLines.some(endsWithCR),
  });
}
