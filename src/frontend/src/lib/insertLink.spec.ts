import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { insertLinkTransaction } from "./insertLink";

function insertLink(doc: string, from = 0, to = doc.length) {
  const state = EditorState.create({ doc, selection: { anchor: from, head: to }, extensions: [markdown()] });
  const updated = state.update(insertLinkTransaction(state)).state;
  return {
    text: updated.doc.toString(),
    selected: updated.sliceDoc(updated.selection.main.from, updated.selection.main.to),
    cursor: updated.selection.main.head,
  };
}

describe("insert link", () => {
  it.each(["https://example.com", "http://example.com", "HTTPS://example.com"])(
    "uses %s as both fields and selects the description", (url) => {
      expect(insertLink(url)).toMatchObject({ text: `[${url}](${url})`, selected: url, cursor: url.length + 1 });
    },
  );

  it.each(["Example", "example.com", "notes.md", "https://example.com and more"])(
    "keeps %s as a description and selects the destination", (text) => {
      expect(insertLink(text)).toMatchObject({ text: `[${text}](url)`, selected: "url" });
    },
  );

  it("starts an empty link in its description", () => {
    expect(insertLink("", 0, 0)).toEqual({ text: "[](url)", selected: "", cursor: 1 });
  });

  it.each([0, 3, 10, 20])("edits an existing destination from position %s", (cursor) => {
    const doc = '[**Example**](https://example.com "Title")';
    expect(insertLink(doc, cursor, cursor)).toMatchObject({ text: doc, selected: "https://example.com" });
  });

  it("edits a selected link without wrapping it", () => {
    const doc = "[Example](<https://example.com/a(b)>)";
    expect(insertLink(doc)).toMatchObject({ text: doc, selected: "https://example.com/a(b)" });
  });

  it("fills an existing empty destination", () => {
    expect(insertLink("[Example]()", 3, 3)).toEqual({ text: "[Example]()", selected: "", cursor: 10 });
  });

  it("selects a reference target without removing its brackets", () => {
    expect(insertLink("[Example][ref]", 3, 3)).toMatchObject({ text: "[Example][ref]", selected: "ref" });
  });

  it("preserves URL punctuation as valid Markdown", () => {
    expect(insertLink("https://example.com/a(b)?q=[x]")).toMatchObject({
      text: "[https://example.com/a(b)?q=\\[x\\]](<https://example.com/a(b)?q=[x]>)",
    });
  });
});
