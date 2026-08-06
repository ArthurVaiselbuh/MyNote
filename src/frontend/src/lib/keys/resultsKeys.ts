import * as act from "../actions";
import { clampIndex } from "../listIndex";
import { peekCtl } from "../paneCtl";
import { app } from "../state/app.svelte";
import { commandFor } from "./bindings";

export function resultsKeys(e: KeyboardEvent) {
  switch (commandFor("results", e)) {
    case "results.selectUp":
      app.resultsSel = clampIndex(app.resultsSel - 1, app.results.length);
      app.focus = "results";
      break;
    case "results.selectDown":
      app.resultsSel = clampIndex(app.resultsSel + 1, app.results.length);
      app.focus = "results";
      break;
    case "results.open":
      act.openResult(app.resultsSel);
      break;
    case "results.refine":
      app.focus = "search";
      app.searchFocusReq++;
      break;
    case "results.nextMatch":
      peekCtl.current?.findNext();
      break;
    case "results.prevMatch":
      peekCtl.current?.findPrev();
      break;
    default:
      return;
  }
  e.preventDefault();
}
