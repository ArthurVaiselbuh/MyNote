import * as act from "../actions";
import { peekCtl } from "../peekCtl";
import { app } from "../state/app.svelte";

export function resultsKeys(e: KeyboardEvent) {
  const handled = () => e.preventDefault();
  switch (e.key) {
    case "ArrowUp":
      handled();
      app.resultsSel = Math.max(0, app.resultsSel - 1);
      app.focus = "results";
      return;
    case "ArrowDown":
      handled();
      app.resultsSel = Math.min(Math.max(0, app.results.length - 1), app.resultsSel + 1);
      app.focus = "results";
      return;
    case "Enter":
      handled();
      act.openResult(app.resultsSel);
      return;
    case "/":
      handled();
      app.focus = "search";
      app.searchFocusReq++;
      return;
    case "n":
      handled();
      peekCtl.current?.findNext();
      return;
    case "p":
      handled();
      peekCtl.current?.findPrev();
      return;
  }
}
