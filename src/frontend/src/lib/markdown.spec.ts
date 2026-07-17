import { describe, expect, it } from "vitest";
import { COLOR_PALETTE, renderBody } from "./markdown";

describe("image width {width=N}", () => {
  it("sets the width attribute and consumes the block", () => {
    const html = renderBody("![shot](pic.png){width=420}");
    expect(html).toContain('width="420"');
    expect(html).not.toContain("{width=420}");
    expect(html).toContain('alt="shot"');
  });

  it("combines with the assets/ src rewrite", () => {
    const html = renderBody("![shot](assets/p1/img.png){width=120}");
    expect(html).toMatch(/src="[^"]*\/p1\/img\.png"/);
    expect(html).toContain('width="120"');
  });

  it("works inside a table cell", () => {
    const html = renderBody(
      "| A | B |\n| --- | --- |\n| ![i](x.png){width=99} | y |",
    );
    expect(html).toContain("<table>");
    expect(html).toContain('width="99"');
    expect((html.match(/<td>/g) ?? []).length).toBe(2);
  });

  it("renders literally when not directly after an image", () => {
    expect(renderBody("plain {width=420}")).toContain("{width=420}");
    expect(renderBody("![i](x.png) {width=420}")).toContain("{width=420}");
    expect(renderBody("[link](x){width=420}")).toContain("{width=420}");
  });

  it("renders literally for invalid widths", () => {
    for (const bad of ["{width=abc}", "{width=0}", "{width=12345}", "{width=50%}", "{width=}"]) {
      const html = renderBody(`![i](x.png)${bad}`);
      expect(html).toContain(bad);
      expect(html).not.toContain("width=\"");
    }
  });

  it("last width wins when repeated", () => {
    const html = renderBody("![i](x.png){width=100}{width=200}");
    expect(html).toContain('width="200"');
    expect(html).not.toContain('width="100"');
  });
});

describe("color span [text]{...}", () => {
  it("renders palette classes via inline style", () => {
    const html = renderBody("a [warn]{.red} b");
    expect(html).toContain(`<span style="color:${COLOR_PALETTE.red}">warn</span>`);
  });

  it("renders every palette name", () => {
    for (const [name, hex] of Object.entries(COLOR_PALETTE)) {
      expect(renderBody(`[x]{.${name}}`)).toContain(`color:${hex}`);
    }
  });

  it("accepts style form with 3/4/6/8-digit hex and both quote types", () => {
    for (const hex of ["abc", "abcd", "a1b2c3", "a1b2c3d4"]) {
      expect(renderBody(`[x]{style="color:#${hex}"}`)).toContain(`color:#${hex}`);
      expect(renderBody(`[x]{style='color:#${hex}'}`)).toContain(`color:#${hex}`);
    }
  });

  it("tokenizes inner markdown", () => {
    const html = renderBody("[a **b** c]{.blue}");
    expect(html).toContain("<strong>b</strong>");
    expect(html).toContain(`color:${COLOR_PALETTE.blue}`);
  });

  it("supports nested spans", () => {
    const html = renderBody("[a [b]{.red} c]{.blue}");
    expect(html).toContain(`color:${COLOR_PALETTE.blue}`);
    expect(html).toContain(`color:${COLOR_PALETTE.red}`);
  });

  it("renders literally for anything outside the whitelist", () => {
    for (const bad of [
      "[x]{.notacolor}",
      '[x]{style="color:red"}',
      '[x]{style="color:#ggg"}',
      '[x]{style="color:#abc;background:url(a)"}',
      '[x]{style="color:#abc" onclick="x"}',
      "[x]{.red .blue}",
      "[x]",
      "[x]{}",
    ]) {
      const html = renderBody(bad);
      expect(html).not.toContain("<span");
    }
  });

  it("renders literally when the bracket is unclosed", () => {
    expect(renderBody("[x{.red}")).not.toContain("<span");
  });

  it("never echoes input into the style attribute", () => {
    const html = renderBody('[x]{style="color:#abc"}');
    expect(html).toContain('<span style="color:#abc">x</span>');
  });

  it("leaves plain links and images untouched", () => {
    expect(renderBody("[a](b)")).toContain('<a href="b">a</a>');
    expect(renderBody("![a](b.png)")).toContain('<img src="b.png" alt="a">');
  });

  it("adds the mod+click hint only to external links", () => {
    expect(renderBody("[a](https://example.com)")).toMatch(
      /<a href="https:\/\/example\.com" title="[^"]*\+Click to open in browser">/,
    );
    expect(renderBody("[a](b.md)")).toContain('<a href="b.md">a</a>');
  });

  it("still renders a colored span when used as link text", () => {
    const html = renderBody("[[x]{.red}](url)");
    expect(html).toContain('<a href="url">');
    expect(html).toContain(`color:${COLOR_PALETTE.red}`);
  });
});

describe("preserveBlankRuns non-regression", () => {
  it("keeps spacer paragraphs around the new syntaxes", () => {
    const html = renderBody("[a]{.red}\n\n\n\n![i](x.png){width=50}");
    expect((html.match(/ /g) ?? []).length).toBe(2);
    expect(html).toContain(`color:${COLOR_PALETTE.red}`);
    expect(html).toContain('width="50"');
  });
});
