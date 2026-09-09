import { app } from "./state/app.svelte";
import { editorCtl, type EditorCtl } from "./paneCtl";

export async function runNotebookUpdate(update: (editor: EditorCtl | null) => Promise<void>) {
  if (app.interactionBlocked) return;
  const editor = editorCtl.current;
  app.interactionBlocked = true;
  editor?.setEditingBlocked(true);
  try {
    await update(editor);
  } finally {
    app.interactionBlocked = false;
    editor?.setEditingBlocked(app.externalChanges !== null);
    if (editorCtl.current !== editor) {
      editorCtl.current?.setEditingBlocked(app.externalChanges !== null);
    }
  }
}
