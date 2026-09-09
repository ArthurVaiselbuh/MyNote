import { api, type Notebook } from "./api";
import { app } from "./state/app.svelte";
import { editorCtl, type EditorCtl } from "./paneCtl";
import { sectionOfPage } from "./treeUtils";
import { setPageForView } from "./actions";
import { runNotebookUpdate } from "./notebookUpdate";

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
  if (!app.notebook || app.externalChanges) return;
  if (checking || app.interactionBlocked) {
    retryWhenInteractionFinishes();
    return;
  }
  checking = true;
  const root = app.root;
  try {
    const changes = await api.checkExternalChanges();
    if (root !== app.root) return;
    if (app.interactionBlocked) {
      retryWhenInteractionFinishes();
      return;
    }
    if (changes[0] || changes[1]) {
      const editor = editorCtl.current;
      const autoReload = !editor?.hasUnsavedChanges();
      app.externalChanges = changes;
      app.externalChangeError = "";
      editor?.setEditingBlocked(true);
      if (autoReload) await resolveExternalChanges(true, true);
    }
  } catch (error) {
    app.status = String(error);
  } finally {
    checking = false;
  }
}

export async function resolveExternalChanges(reload: boolean, automatic = false) {
  const changes = app.externalChanges;
  if (!changes || app.externalResolution !== "idle") return;
  await runNotebookUpdate(async (editor) => {
    app.externalResolution = automatic ? "automatic" : reload ? "reload" : "overwrite";
    app.externalChangeError = "";
    try {
      const loaded = editor?.printContent();
      const content = loaded ? `# ${loaded.title}\n\n${loaded.body}` : null;
      const [notebook, changedPage] = await api.resolveExternalChanges(reload, content);
      await refreshExternalChanges(notebook, reload && !!(changedPage || changes[1]), editor);
      app.externalChanges = null;
      app.status = "";
      if (!reload && editor && !(await editor.save())) {
        app.externalChanges = changes;
        throw new Error(app.status || "Could not save page");
      }
    } catch (error) {
      app.externalChangeError = String(error);
    } finally {
      app.externalResolution = "idle";
    }
  });
}

async function refreshExternalChanges(notebook: Notebook, reloadPage: boolean, editor: EditorCtl | null) {
  app.notebook = notebook;
  const id = app.currentPageId;
  const section = id ? sectionOfPage(notebook, id) : null;
  const removed = id !== null && !section;
  if (reloadPage || removed) editor?.discardChanges();
  if (removed || (reloadPage && id)) {
    const nextPage = removed ? null : id;
    app.pendingPage = null;
    let loaded = true;
    if (editor) loaded = await editor.load(nextPage, true);
    else if (nextPage) loaded = await setPageForView(nextPage, { allowWhileBlocked: true, forceLoad: true });
    if (!loaded) throw new Error(app.status || "Could not reload page");
    app.currentPageId = nextPage;
    app.selectedId = nextPage;
  }
  app.sectionIdx = section
    ? notebook.sections.indexOf(section)
    : Math.min(app.sectionIdx, Math.max(0, notebook.sections.length - 1));
}
