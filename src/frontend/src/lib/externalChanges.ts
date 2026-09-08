import { api } from "./api";
import { app } from "./state/app.svelte";
import { editorCtl } from "./paneCtl";
import { sectionOfPage } from "./treeUtils";
import { openPageById } from "./actions";

let checking = false;
let retryQueued = false;

function retryWhenInteractionFinishes() {
  if (retryQueued) return;
  retryQueued = true;
  setTimeout(() => {
    retryQueued = false;
    void checkExternalChanges();
  }, 100);
}

export async function checkExternalChanges() {
  if (!app.notebook) return;
  if (checking || app.interactionBlocked) {
    retryWhenInteractionFinishes();
    return;
  }
  checking = true;
  const root = app.root;
  try {
    const changes = await api.checkExternalChanges();
    if (root !== app.root || app.interactionBlocked) return;
    if (changes[0] || changes[1]) {
      app.externalChanges = changes;
      editorCtl.current?.setEditingBlocked(true);
    }
  } catch (error) {
    app.status = String(error);
  } finally {
    checking = false;
  }
}

export async function resolveExternalChanges(reload: boolean) {
  const changes = app.externalChanges;
  if (!changes) return;
  const editor = editorCtl.current;
  const loaded = editor?.printContent();
  const content = loaded ? `# ${loaded.title}\n\n${loaded.body}` : null;
  const [notebook, changedPage] = await api.resolveExternalChanges(reload, content);
  app.notebook = notebook;
  const id = app.currentPageId;
  const removed = id && !sectionOfPage(notebook, id);
  if ((reload && changedPage) || removed) editor?.discardChanges();
  if (removed) {
    await editor?.load(null);
    app.pendingPage = null;
    app.currentPageId = null;
    app.selectedId = null;
  } else if (reload && changedPage && id) {
    app.pendingPage = null;
    if (editor) {
      if (!(await editor.load(id, true))) throw new Error(app.status || "Could not reload page");
    } else {
      await openPageById(id);
    }
  }
  app.sectionIdx = id && !removed
    ? Math.max(0, notebook.sections.findIndex((section) => section.id === sectionOfPage(notebook, id)?.id))
    : Math.min(app.sectionIdx, Math.max(0, notebook.sections.length - 1));
  app.externalChanges = null;
  editor?.setEditingBlocked(false);
  app.status = "";
  if (!reload) await editor?.save();
}
