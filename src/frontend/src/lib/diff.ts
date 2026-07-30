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

const LCS_CELL_CAP = 4_000_000;
const WORD_SPAN_BUDGET = 800;
const MAX_WORD_TOKENS = 400;
const SIMILARITY_THRESHOLD = 0.3;

function splitLines(text: string): { lines: string[]; hadTrailingNewline: boolean } {
  const lines = text.split("\n");
  let hadTrailingNewline = false;
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
    hadTrailingNewline = true;
  }
  return { lines, hadTrailingNewline };
}

function internKey(raw: string, key: Map<string, number>, next: { v: number }): number {
  let id = key.get(raw);
  if (id === undefined) {
    id = next.v++;
    key.set(raw, id);
  }
  return id;
}

function internLines(lines: string[], key: Map<string, number>, next: { v: number }): number[] {
  return lines.map((line) => internKey(line.endsWith("\r") ? line.slice(0, -1) : line, key, next));
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
  if (n === 0 && m === 0) return { ops: [], degraded: false };
  if (n * m > LCS_CELL_CAP) {
    const ops: Op[] = [];
    for (let i = 0; i < n; i++) ops.push("del");
    for (let j = 0; j < m; j++) ops.push("add");
    return { ops, degraded: true };
  }
  const width = m + 1;
  const dp = new Int32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i * width + j] =
        a[i] === b[j] ? dp[(i + 1) * width + (j + 1)] + 1 : Math.max(dp[(i + 1) * width + j], dp[i * width + (j + 1)]);
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
  const key = new Map<string, number>();
  const next = { v: 0 };
  const a = internLines(leftLines, key, next);
  const b = internLines(rightLines, key, next);
  const { prefix, suffix } = trimCommon(a, b);

  const ops: LineOp[] = [];
  for (let k = 0; k < prefix; k++) ops.push({ kind: "same", ai: k, bi: k });

  const aMid = a.slice(prefix, a.length - suffix);
  const bMid = b.slice(prefix, b.length - suffix);
  const { ops: midOps, degraded } = lcsOps(aMid, bMid);
  let ai = prefix;
  let bi = prefix;
  for (const kind of midOps) {
    if (kind === "same") {
      ops.push({ kind: "same", ai, bi });
      ai++;
      bi++;
    } else if (kind === "del") {
      ops.push({ kind: "del", ai });
      ai++;
    } else {
      ops.push({ kind: "add", bi });
      bi++;
    }
  }

  const leftSuffixStart = leftLines.length - suffix;
  const rightSuffixStart = rightLines.length - suffix;
  for (let k = 0; k < suffix; k++) {
    ops.push({ kind: "same", ai: leftSuffixStart + k, bi: rightSuffixStart + k });
  }
  return { ops, degraded };
}

function tokenizeForDiff(line: string): string[] {
  return line.match(/[\p{L}\p{N}_]+|\s+|[^\s\p{L}\p{N}_]/gu) ?? (line.length ? [line] : []);
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

function isSimilar(a: string, b: string): boolean {
  if (a === b) return true;
  return bagSimilarity(tokenizeForDiff(a), tokenizeForDiff(b)) >= SIMILARITY_THRESHOLD;
}

function pushSpan(spans: Span[], text: string, hl: boolean) {
  const last = spans[spans.length - 1];
  if (last && last.hl === hl) {
    last.text += text;
  } else {
    spans.push({ text, hl });
  }
}

function diffWords(a: string, b: string): { left: Span[]; right: Span[] } {
  if (a === b) return { left: [{ text: a, hl: false }], right: [{ text: b, hl: false }] };
  const leftTokens = tokenizeForDiff(a);
  const rightTokens = tokenizeForDiff(b);
  if (leftTokens.length > MAX_WORD_TOKENS || rightTokens.length > MAX_WORD_TOKENS) {
    return { left: [{ text: a, hl: true }], right: [{ text: b, hl: true }] };
  }
  const key = new Map<string, number>();
  const next = { v: 0 };
  const ai = leftTokens.map((t) => internKey(t, key, next));
  const bi = rightTokens.map((t) => internKey(t, key, next));
  const { ops } = lcsOps(ai, bi);

  const left: Span[] = [];
  const right: Span[] = [];
  let li = 0;
  let ri = 0;
  for (const kind of ops) {
    if (kind === "same") {
      pushSpan(left, leftTokens[li], false);
      pushSpan(right, rightTokens[ri], false);
      li++;
      ri++;
    } else if (kind === "del") {
      pushSpan(left, leftTokens[li], true);
      li++;
    } else {
      pushSpan(right, rightTokens[ri], true);
      ri++;
    }
  }
  return { left, right };
}

function diffWordsBudgeted(a: string, b: string, changedSoFar: number): { left: Span[]; right: Span[] } {
  if (changedSoFar >= WORD_SPAN_BUDGET) {
    return { left: [{ text: a, hl: true }], right: [{ text: b, hl: true }] };
  }
  return isSimilar(a, b) ? diffWords(a, b) : { left: [{ text: a, hl: true }], right: [{ text: b, hl: true }] };
}

export function diffText(leftText: string, rightText: string): DiffResult {
  const { lines: leftLines, hadTrailingNewline: leftTrail } = splitLines(leftText);
  const { lines: rightLines, hadTrailingNewline: rightTrail } = splitLines(rightText);
  const leftHasCR = leftLines.some((l) => l.endsWith("\r"));
  const rightHasCR = rightLines.some((l) => l.endsWith("\r"));

  const { ops, degraded } = buildLineOps(leftLines, rightLines);

  const rows: AlignedRow[] = [];
  const unified: UnifiedRow[] = [];
  const anchors: number[] = [];
  const unifiedAnchors: number[] = [];
  let added = 0;
  let removed = 0;
  let changed = 0;

  let i = 0;
  while (i < ops.length) {
    const op = ops[i];
    if (op.kind === "same") {
      const l = leftLines[op.ai];
      const r = rightLines[op.bi];
      rows.push({ kind: "same", leftNo: op.ai + 1, left: [{ text: l, hl: false }], rightNo: op.bi + 1, right: [{ text: r, hl: false }] });
      unified.push({ kind: "same", leftNo: op.ai + 1, rightNo: op.bi + 1, spans: [{ text: l, hl: false }] });
      i++;
      continue;
    }

    const dels: number[] = [];
    const adds: number[] = [];
    while (i < ops.length && ops[i].kind !== "same") {
      const o = ops[i];
      if (o.kind === "del") dels.push(o.ai);
      else if (o.kind === "add") adds.push(o.bi);
      i++;
    }
    anchors.push(rows.length);
    unifiedAnchors.push(unified.length);

    const pairCount = Math.min(dels.length, adds.length);
    for (let k = 0; k < pairCount; k++) {
      const li = dels[k];
      const ri = adds[k];
      const { left, right } = diffWordsBudgeted(leftLines[li], rightLines[ri], changed);
      rows.push({ kind: "change", leftNo: li + 1, left, rightNo: ri + 1, right });
      unified.push({ kind: "del", leftNo: li + 1, rightNo: null, spans: left });
      unified.push({ kind: "add", leftNo: null, rightNo: ri + 1, spans: right });
      changed++;
    }
    for (let k = pairCount; k < dels.length; k++) {
      const li = dels[k];
      rows.push({ kind: "del", leftNo: li + 1, left: [{ text: leftLines[li], hl: true }], rightNo: null, right: null });
      unified.push({ kind: "del", leftNo: li + 1, rightNo: null, spans: [{ text: leftLines[li], hl: true }] });
      removed++;
    }
    for (let k = pairCount; k < adds.length; k++) {
      const ri = adds[k];
      rows.push({ kind: "add", leftNo: null, left: null, rightNo: ri + 1, right: [{ text: rightLines[ri], hl: true }] });
      unified.push({ kind: "add", leftNo: null, rightNo: ri + 1, spans: [{ text: rightLines[ri], hl: true }] });
      added++;
    }
  }

  return {
    rows,
    unified,
    anchors,
    unifiedAnchors,
    stats: { added, removed, changed },
    degraded,
    trailingNewlineChanged: leftTrail !== rightTrail,
    lineEndingsDiffer: leftHasCR !== rightHasCR,
  };
}
