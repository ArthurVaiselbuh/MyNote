# MyNote

Local-first, keyboard-driven, OneNote-style Markdown note app. Windows is the
primary target (built for a locked-down corporate environment); macOS and Linux
are supported cross-platform builds off the same code.

> **Maintenance rule:** this file records *requirements and design decisions*.
> Update it **only when a decision changes** — never as part of routine code
> changes. The code is the source of truth for how things work; this file is
> the source of truth for *why* and for constraints that must not be broken.

## Hard requirements

- **No runtime network.** No localhost server, no ports, no CDN, no external
  requests. Build-time network (npm/cargo) is fine.
- **Single small binary**, no installer. NSIS/bundling stays disabled on every
  platform (`tauri build --no-bundle`). The runtime webview dependency is the
  system one: WebView2 (pre-installed on Win10/11), WKWebView (bundled with
  macOS), and webkit2gtk on Linux — the only platform where the user/CI must
  install it (`libwebkit2gtk-4.1`). No app-shipped runtime otherwise.
- Notes are **plain Markdown files on disk**, AI-agent friendly: one
  `<uuid>.md` per page, first line is the `# Title` H1.

## Layout & build

- Single `src/` folder: `src/frontend` (Svelte 5 runes + Vite + CodeMirror 6),
  `src/backend` (Tauri v2 Rust crate). Wired via `src/backend/tauri.conf.json`
  (`frontendDist: ../frontend/dist`).
- Root release scripts: `build.bat` (Windows) and `build.sh` (macOS/Linux);
  `dev.bat`/`dev.sh` run `tauri dev`, `test.bat`/`test.sh` run the checks. They
  must go through the tauri CLI (`tauri build --no-bundle`): plain
  `cargo build --release` yields a dev-mode binary that tries to load the vite
  devUrl and shows a blank window.
- Output (a root-level `output\` folder alongside `src`): `build.bat` copies the
  release exe to `output\MyNote.exe`; `build.sh` copies to `output\mynote-macos`
  or `output\mynote-linux`. The raw cargo output stays at
  `src\backend\target\release\MyNote`.
- Config/data dir is per-platform (see On-disk format): backend derives it in
  `settings.rs::config_root()`, so no code hardcodes `%APPDATA%`.

## On-disk format (stable contract — breaking it breaks user notebooks)

- Notebook root defaults to `<config>\MyNote\notebook`, where `<config>` is
  `%APPDATA%` (Windows), `~/Library/Application Support` (macOS), or
  `$XDG_CONFIG_HOME`/`~/.config` (Linux) — see `settings.rs::config_root()`. Any
  folder can be opened via Ctrl+O.
- `notebook.json` — section list + nested page tree (`id`, `title`,
  `expanded`, `children`) + last view. Page titles are cached here but the
  H1 in the `.md` file wins; they are kept in sync on write/rename.
- `notebook.json` is the only tree metadata file.
- `assets/<page-id>/` — pasted images. Orphans are pruned **only on notebook
  close/switch**, never during a session (preserves editor undo).
- **Deletes are deferred:** deleting a page/section drops it from
  `notebook.json` immediately, but the `.md` files (and assets) stay on disk
  until clean close/notebook switch so tree undo can restore them. Only ids
  the app deleted in the current session are purged at close — unreferenced
  files it didn't delete are never touched.
- App settings (theme colors, zoom, window geometry, scroll speed, focus
  alpha, last notebook path, recent-notebooks MRU) live in
  `<config>\MyNote\settings.json` (same `config_root()` as above), not in the
  notebook. Window geometry and the MRU are backend-owned: `set_settings`
  ignores the frontend's copies.

## Design decisions

- **Images** are served through a custom `note-asset:` URI scheme registered in
  Rust — no asset-protocol scope juggling, no localhost server. The webview
  resolves the scheme differently per platform: `http://note-asset.localhost/`
  on Windows, `note-asset://localhost/` on macOS/Linux. `markdown.ts` picks the
  base by userAgent (same rule Tauri uses); the Rust handler reads only the URL
  path so it's platform-agnostic. CSP `img-src` must keep **both**
  `http://note-asset.localhost` and the `note-asset:` scheme, and `connect-src`
  must keep `ipc: http://ipc.localhost` (the `ipc:` scheme covers the
  macOS/Linux `ipc://localhost` form) or Tauri IPC degrades to the slow
  postMessage fallback.
- **Open/switch notebook** (Ctrl+O) is an in-app pane, not a bare OS folder
  picker: logo, the 5 most recent notebooks (missing ones shown but inert),
  plus New (folder picker; refuses a folder that already has a
  `notebook.json`) and Open (native picker selecting the `notebook.json`
  itself, so only real notebooks can be opened — the backend maps that file
  path to its parent folder).
- **Backend owns all file I/O** behind Tauri commands with
  `State<Mutex<Option<Store>>>`. Frontend mutates via commands, then re-fetches
  the tree (`get_tree`) — no client-side tree bookkeeping beyond display state.
- **`move_page` index contract:** the index is the position in the destination
  list counted *after* the page has been detached. Frontend computes indices
  against sibling lists with the dragged node filtered out.
- **Title sync is bidirectional:** editing the H1/title field renames the tree
  node (`write_page` returns the derived title); tree rename (F2) rewrites the
  H1 in the file.
- **Search** runs in Rust: fuzzy via `nucleo-matcher`, regex via `regex`,
  scanning titles + bodies on demand (no persistent index). Hits carry char
  ranges for snippet highlighting.
- **Theme:** dark only, but all colors (text/background/panel/accent), focus
  alpha, and scroll speed are user-configurable in Settings and applied as CSS
  variables on the app root.
- **Blank lines are WYSIWYG in the preview:** files keep the exact newlines
  typed in the editor; `renderBody` pre-transforms runs of 2+ blank lines into
  nbsp spacer paragraphs (code fences excluded). Deliberately nonstandard —
  other Markdown renderers collapse those runs — chosen so the file stays
  clean plain Markdown instead of littering it with `&nbsp;`/`<br>` tokens.
  The importer preserves OneNote's empty paragraphs as extra newlines.
- **OneNote import** (Ctrl+I, `import_mht.rs`): parses OneNote single-file
  exports (.mht) in Rust — hand-rolled MIME/quoted-printable/HTML handling, no
  new deps. Each file becomes a *new* section named after the file; pages split
  on top-level `border-width:100%` divs (20pt paragraph = title, gray date
  block → italic stamp line); multipart image parts land in
  `assets/<page-id>/`. A preview modal warns about duplicate section/page names
  (vs the notebook and within the batch) before importing — never auto-merges.
  Markdown specials in note text are not escaped (fidelity over rendering).
- **Tree undo/redo** (Ctrl+Z/Y, backend `Store` op stack): covers delete
  page/section and move page (reorder, demote/promote, cross-section) —
  not creates/renames. Ops record their inverse (detached subtree + location),
  so interleaved edits/renames survive an undo; relies on deferred file
  deletion for content. While typing, Ctrl+Z/Y stay native (CodeMirror/input
  undo) — the tree stack only owns them outside text fields.
- **Deferred / out of scope:** light theme, installer.

## Keyboard & focus model (the hard part — keep the precedence exact)

Four logical focus targets: `tree | editor | search | results`. Focus is
app-managed state (`app.focus`), not raw DOM focus; the focused pane gets an
outline of accent color at `--focus-alpha`.

"Ctrl+X" below means the **primary modifier**: Ctrl on Windows/Linux, Cmd
(⌘) on macOS. `keys/platform.ts` owns the seam — `modPressed(e)` gates the
dispatcher (`dispatch.ts`, `treeKeys.ts`) and `MOD_LABEL`/`ALT_LABEL` feed the
one shortcut table in `keys/shortcuts.ts` that both the help overlay and every
tooltip render from. Intended as the basis for future user-configurable
keybindings: change/extend chords in those two files, not scattered literals.

One capture-phase window keydown listener dispatches with strict precedence:

1. Open modal (picker/help/settings/confirm/insert helper) owns the keyboard;
   only Esc is handled globally (closes it).
2. Global shortcuts fire even while typing: Ctrl+K/F/E/S/N/Shift+N/1/2/3/
   G/Shift+G/O/I/J/, · Ctrl+PgUp/Dn · Ctrl+=/−/0 · F3/Shift+F3 (`?` only when
   not typing). Ctrl+G opens the section picker in go-to mode, Ctrl+Shift+G
   in move-page mode; cross-section moves follow the page (switch to the
   target section with the page selected). Ctrl+Z/Y (tree undo/redo) are
   global but yield to native text undo while typing.
3. Typing guard: inside input/textarea/contentEditable/CodeMirror, pane-local
   keys are suppressed. Exception: Tab still cycles out of the search box;
   Tab inside CodeMirror indents.
4. Pane-local keys (tree: arrows, `/`, Enter, F2, Del, Ctrl+]/[,
   Alt+↑/↓ reorder, Alt+←/→ move to section — tree moves all use Alt+arrows;
   results: arrows, Enter, `/`).

Esc peels one layer per press: modal → find panel → results→page →
editor→tree → clear tree filter. Tab cycles a view-dependent ring
(page: tree↔editor; results: tree→search→results). PgUp/PgDn scroll the main
view even when focus is elsewhere (CodeMirror keeps native paging with the
caret). The `?` overlay is context-aware: focused-pane keys, pane keys,
globals — filterable.

Gotcha: the Editor component unmounts in results view — anything that needs
the editor after leaving results must go through app state consumed on mount
(e.g. `app.findPrefill`), not `editorCtl` calls. The `*FocusReq` counters are
also consumed on mount: their effects must stay gated on `app.focus` matching
the pane, or a remount replays a stale request and steals focus (broke the
results Tab ring once). Related: `focusPane("tree")` must not force the page
view, or the results ring could never cycle through the tree.

## Testing

- `cargo test` in `src/backend` covers store tree ops, title sync, search,
  undo/redo, and asset/file pruning — keep these green and extend them for
  new backend logic. This is the cross-platform test bar: CI runs it on
  Windows, macOS, and Linux.
- E2e regression suite: **Windows only** — Playwright in `src/e2e` (`npm test`
  there, or root `test.bat` for backend + e2e). It attaches to the **real
  release exe** (`output\MyNote.exe`, so `build.bat` first) over CDP via
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` — no
  browser downloads, no webdriver binary, keeps the no-runtime-network rule
  intact. CDP-over-WebView2 has no macOS/Linux equivalent, so `test.sh` runs
  backend + svelte-check only and CI runs e2e on the Windows leg alone. Each test gets a scratch notebook + isolated
  `WEBVIEW2_USER_DATA_FOLDER`; the user's `settings.json` is swapped and
  restored by the fixture (`app.ts`). Serial only (one exe, one port); the
  suite refuses to run while MyNote is open. Extend it for new keyboard/UI
  flows — keyboard races matter: wait for the rAF-driven title focus before
  sending the next key (see `newPageToTree`).
