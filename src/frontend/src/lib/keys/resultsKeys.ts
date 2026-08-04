import * as act from "../actions";
import { peekCtl } from "../peekCtl";
import { app } from "../state/app.svelte";
import { commandFor } from "./bindings";

export function resultsKeys(e: KeyboardEvent) {
  switch (commandFor("results", e)) {
    case "results.selectUp":
      app.resultsSel = Math.max(0, app.resultsSel - 1);
      app.focus = "results";
      break;
    case "results.selectDown":
      app.resultsSel = Math.min(Math.max(0, app.results.length - 1), app.resultsSel + 1);
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
