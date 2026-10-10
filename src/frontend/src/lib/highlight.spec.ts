import { describe, expect, it } from "vitest";
import { applyHighlights, clearHighlights } from "./highlight";

function highlight(text: string, expression: RegExp, lineByLine = true) {
  const container = document.createElement("pre");
  container.textContent = text;
  const matches = applyHighlights(container, expression, lineByLine);
  expect(container.textContent).toBe(text);
  return { container, matches };
}

describe("attachment line highlighting", () => {
  it.each(["\n", "\r\n"])("matches anchored interior lines with %j separators", (newline) => {
    const text = ["first", "", "needle", "needles", "needle", "last", ""].join(newline);
    const { container, matches } = highlight(text, /^needle$/gi);
    expect(matches.map((match) => match.textContent)).toEqual(["needle", "needle"]);
    clearHighlights(container);
    expect(container.textContent).toBe(text);
    expect(container.querySelector("mark")).toBeNull();
  });

  it("preserves offsets after Unicode text and multiple matches on each line", () => {
    const { matches } = highlight("😀 needle needle\r\nעברית needle needle", /needle/g);
    expect(matches).toHaveLength(4);
    expect(matches.every((match) => match.textContent === "needle")).toBe(true);
  });

  it.each([/needle\s+tail/g, /needle[\s\S]+tail/g, /needle.*tail/gs])(
    "does not match across line boundaries with %s", (expression) => {
      expect(highlight("needle\ntail\r\nneedle tail", expression).matches.map((match) => match.textContent))
        .toEqual(["needle tail"]);
    },
  );

  it("excludes line terminators but preserves a lone carriage return", () => {
    const { matches } = highlight("\r\n\nneedle\r\n\r", /\s/g);
    expect(matches.map((match) => match.textContent)).toEqual(["\r"]);
  });

  it("skips empty matches without losing later nonempty matches", () => {
    expect(highlight("\nneedle\r\n\nneedle\n", /^$|needle|$/g).matches).toHaveLength(2);
    expect(highlight("\n\r\n", /^$/g).matches).toHaveLength(0);
    expect(highlight("", /^$/g).matches).toHaveLength(0);
  });

  it("keeps markup in the attachment as literal text", () => {
    const { container, matches } = highlight("<img src=x> & needle\n<script>needle</script>", /needle/g);
    expect(matches).toHaveLength(2);
    expect(container.querySelector("img, script")).toBeNull();
  });

  it("preserves whole-text matching by default for page previews", () => {
    const container = document.createElement("pre");
    container.textContent = "first\nneedle\ntail";
    expect(applyHighlights(container, /^needle$/g)).toHaveLength(0);
    expect(applyHighlights(container, /needle\s+tail/g).map((match) => match.textContent))
      .toEqual(["needle\ntail"]);
  });
});
