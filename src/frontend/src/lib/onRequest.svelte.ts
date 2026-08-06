import { untrack } from "svelte";

/** Runs `run` whenever the request counter `read` returns changes value.
 * Seeded at 0 rather than at the counter's current value, so a pane that mounts
 * late still sees a request raised before it existed — which is why every `run`
 * that targets a pane also has to check that the request was aimed at it. */
export function onRequest(read: () => number, run: () => void) {
  let handled = 0;
  $effect(() => {
    const req = read();
    if (req === handled) return;
    handled = req;
    untrack(run);
  });
}
