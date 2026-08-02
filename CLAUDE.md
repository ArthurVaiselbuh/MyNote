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
  `expanded`, `children`) . Page titles are cached here but the
  H1 in the `.md` file wins; they are kept in sync on write/rename.
- `notebook.json` is the only tree metadata file; `notebook.json.bak` is a
  crash-recovery copy `open` falls back to. All file writes go through
  temp-file-plus-rename (`store::atomic_write`) so a crash can't truncate them.
- `notebook.user.json` — **volatile per-user view state, not version controlled
- `assets/<page-id>/` — pasted images. Orphans are pruned **only on notebook
  close/switch**, never during a session (preserves editor undo).
- **Deletes are deferred:** deleting a page/section drops it from
  `notebook.json` immediately, but the `.md` files (and assets) stay on disk
  until clean close/notebook switch so tree undo can restore them. Only ids
  the app deleted in the current session are purged at close — unreferenced
  files it didn't delete are never touched.
- App settings (theme colors, zoom, window geometry, etc.) live in
  `<config>\MyNote\settings.json` (same `config_root()` as above), not in the
  notebook. Window geometry and the MRU are backend-owned: `set_settings`
  ignores the frontend's copies.
- `<config>\MyNote\MyNote.log` sits alongside `settings.json`.
- `notebook.json`'s `git` key is the per-notebook opt-in for git snapshots — see the design decision below.

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
- **One notebook, one writer:** `Store::open` takes an OS-level exclusive lock
  on `.mynote.lock` in the notebook root, held for the `Store`'s lifetime.
- **Title sync is bidirectional:** editing the H1/title field renames the tree
  node (`write_page` returns the derived title); tree rename (F2) rewrites the
  H1 in the file.
- **Search** runs in Rust over titles + bodies on demand (no persistent index);
  hits carry char ranges for snippet highlighting. Search uses a ranking - strategy system to attempt
  finding the best match/page
- **Opening a search result lands in preview**, not the editor, with the
  preview's find prefilled by an alternation of the parsed terms (the raw
  pattern in regex mode) so every keyword lights up at once.
- **The results view peeks the selected hit** (`ResultPeek.svelte`): the list
  keeps the left of the main pane, a read-only render of the selected page takes
  the right, behind its own splitter (`settings.json::peekWidth`). It is inert —
  no `app.focus` target of its own, no find panel, links are dead — so the
  four-pane focus model and the Esc ladder are unchanged. It reads its page with
  `read_page` debounced 120ms (arrow-key traversal outruns the reads) and cached
  per result set. It does **not** scroll to the hit's `lineNo`: `renderBody`'s
  blank-line spacer paragraphs destroy any source-line-to-DOM mapping, so it
  scrolls to the first highlighted term instead, which is what was being looked
  for anyway. Highlighting is the same DOM pass the preview's find uses
  (`lib/highlight.ts`). Unlike `Preview`, the peek writes its body with
  `innerHTML` rather than `{@html}` — highlighting rewrites text nodes under the
  node, which leaves Svelte's bookkeeping for its own range stale and silently
  drops the body on the next update.
- **Theme:** dark only, but all colors (text/background/panel/accent), focus
  alpha, and scroll speed are user-configurable in Settings and applied as CSS
  variables on the app root.
- **Blank lines are WYSIWYG in the preview:** files keep the exact newlines
  typed in the editor; `renderBody` pre-transforms runs of 2+ blank lines into
  nbsp spacer paragraphs (code fences excluded). Deliberately nonstandard —
  other Markdown renderers collapse those runs — chosen so the file stays
  clean plain Markdown instead of littering it with `&nbsp;`/`<br>` tokens.
  The importer preserves OneNote's empty paragraphs as extra newlines.
- **Import** (Ctrl+I) is a unified in-app pane (`Import.svelte`, an `app.modal`
  like Open Notebook) with two sources; both preview a tree of the
  sections/pages that will be created — flagging duplicate titles and
  colliding section names (vs the notebook and within the batch) — and always
  add *new* sections, never auto-merge. Shared preview/outcome types and the
  duplicate tracker live in `import.rs`.
  - **OneNote** (`import_mht.rs`): parses single-file exports (.mht) in Rust —
    hand-rolled MIME/quoted-printable/HTML handling, no new deps. Each file
    becomes a section named after the file; pages split on top-level
    `border-width:100%` divs (20pt paragraph = title, gray date block → italic
    stamp line); multipart image parts land in `assets/<page-id>/`. Markdown
    specials in note text are not escaped (fidelity over rendering).
  - **Markdown folder** (`import_md.rs`): copies a folder of `.md`/`.markdown`
    files into the notebook as fresh `<uuid>.md` pages (never referenced in
    place). Top-level subfolders become sections; deeper subfolders become
    nested parent pages (sections can't nest) whose body is their
    `README.md`/`index.md` if present else a `# <folder>` stub; loose top-level
    files go into one section named after the chosen folder. Titles come from
    each file's H1 (else its filename), normalized to a leading H1 via
    `store::set_title`. Folders with no markdown anywhere are skipped; relative
    image links are copied verbatim (no asset rewriting).
- **Inline formatting beyond CommonMark uses Pandoc attribute syntax**,
  whitelisted (markdown-it stays `html:false`): `![alt](src){width=420}` sets
  image width (px, 1–9999, still capped by the preview width) and
  `[text]{.red}` / `[text]{style="color:#hex"}` colors text (8 palette names
  in `markdown.ts::COLOR_PALETTE`; 3/4/6/8-digit hex). Anything else renders
  literally. Chosen for on-disk consistency over Obsidian-style `|420`/HTML
  spans; Pandoc/Quarto render both forms natively.
- **External links** in the preview open in the OS default browser on
  Ctrl/⌘+Click only (plain click is inert; the link's title tooltip hints the
  chord) via `tauri-plugin-opener`; the capability grants only `opener:allow-open-url`
  scoped to `http://*`/`https://*` (deliberately not `opener:default` — no
  mailto/tel/reveal-in-dir). The webview itself never navigates away: a
  `navigation-guard` plugin in `lib.rs` rejects main-frame navigation outside
  the app origin (`tauri:`/`note-asset:` schemes, `localhost`/`*.localhost`
  hosts). This keeps the no-runtime-network rule — the app process makes no
  requests; opening a link is delegated to the user's browser.
- **Tree undo/redo** (Ctrl+Z/Y, backend `Store` op stack): covers delete
  page/section and move page (reorder, demote/promote, cross-section) —
  not creates/renames. Ops record their inverse (detached subtree + location),
  so interleaved edits/renames survive an undo; relies on deferred file
  deletion for content. While typing, Ctrl+Z/Y stay native (CodeMirror/input
  undo) — the tree stack only owns them outside text fields.
- **Logging** goes through `tauri-plugin-log` (the built-in Tauri mechanism)
  into a single `MyNote.log` file in `app_dir()`, size-capped with
  `RotationStrategy::KeepOne`. The level
  comes from `settings.json::logLevel` (`off|error|warn|info|verbose`, — see `settings.rs::log_level_filter`, where
  `verbose` maps to `Trace`) and is fixed at startup, so changing it needs a
  restart. Frontend logs (`log.ts`) share the same file and level filter via
  the plugin's IPC.

  Convention:
  - **info** = content mutations (create/move/
  delete/rename page & section, undo/redo, notebook open, import);
  - **verbose (trace)** = UI events and high-frequency I/O (focus switches, dialog opens,
  page read/write, search, expand toggles).
  - Logs are added only to existing code paths — no handlers exist solely to log.
  - **No user content or personal data in logs:** never log note bodies,
    page/section titles, search query text, or filesystem paths. Log ids
    (UUIDs), counts, sizes, indices, and enum/pane names only.
- **Autosave** debounces 3s after the last keystroke
- **Per-notebook git snapshots** are an opt-in safety net (`notebook.json`'s
  `git.enabled`/`git.intervalSecs`, toggle in Settings), never a runtime
  dependency: MyNote shells out to the system `git`, and
  silently no-ops everywhere if it isn't on `PATH` (checked once, cached).
  Every invocation runs with the user's global and system git config ignored
  (`GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` nulled in `git.rs::base()`), so only
  the repo-local config MyNote writes applies — a globally installed git-lfs
  filter or credential helper can't rewrite the stored bytes or stall a commit.
  Git is never on the data-safety path — every git failure is "skip and log".
- **History pane** (Ctrl+H page revisions / Ctrl+Shift+H deleted pages,
  `history.rs`) reads those snapshots back and is **hidden entirely when git
  isn't available** — both shortcuts and the Settings "Browse history…"
  button gate on `GitStatus.available` (`app.git` in the frontend). It is a
  **full-window pane, not a centered modal**: the revision rail takes over the
  tree's side and the diff the preview's, so diffs get the whole screen. It
  still lives in `app.modal` (so the keyboard-precedence rules above apply
  unchanged) and the tree/`Editor` stay mounted underneath — the Editor *must*
  stay mounted or `editorCtl.replaceAll` has nothing to restore into. It also
  stays mounted behind its own restore/recover confirm
  (`app.confirm.returnTo === "history"`), so cancelling returns to an intact
  pane; the same holds for the `?` cheat sheet it opens. Its window key handler
  bails whenever `app.modal !== "history"`, so whichever modal it opened owns
  the keyboard.
  The diff's **base defaults to "now (on disk)" with the newest snapshot
  selected**, so base is the before side and the selection the after side:
  the split view reads as *what restoring the selected revision would do*
  (red = lines it drops, green = lines it brings back). Base is movable
  (Shift+↑/↓, `B`). Commit shas are deliberately not shown — the rail
  identifies a revision by timestamp and relative age. All git
  use here is read-only (`log`, `cat-file`, `ls-tree` in `git.rs`, batched via
  `cat-file --batch` so listing costs a handful of processes regardless of
  history size) with deadlines and caps, so reads take no `git_gate` and never
  block a snapshot — they take a separate `history_gate` that only bounds
  concurrent process spawns. Every history command clones the `root`/
  `notebook` it needs and drops the store lock before running git, so
  browsing history never blocks autosave/typing on the same store mutex.
  Diffs are computed in TypeScript (`lib/diff.ts`, no dependency) rather than
  parsed from `git diff`, so any revision can be compared with any other
  including the live on-disk buffer, and **the whole page is always
  rendered — unchanged runs are never collapsed**, side-by-side or inline.
  Restoring a revision replaces the CodeMirror buffer as one normal edit
  (`editorCtl.replaceAll`, undoable via Ctrl+Z, saved by the usual autosave
  path) — never the on-disk file directly. Recovering a deleted page reads
  its subtree shape from `notebook.json`'s own history (deferred deletion
  means the deleting commit itself often doesn't touch `notebook.json` — see
  the doc comment on `history::deleted_pages`) and writes bodies +
  `assets/<page-id>/` through `Store::insert_restored`, reusing the original
  id when free (else a new id, with asset links rewritten), landing at the
  top level of its old section when an ancestor is gone rather than
  recreating it. A recovery counts as a *create*, so it is not on the tree
  undo stack (matching `create_page`/import).
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
   only Esc is handled globally (closes it; a stacked sub-modal steps back one
   layer instead — Esc in the color picker returns to the insert helper).
2. Global shortcuts fire even while typing: Ctrl+K/F/E/S/N/Shift+N/1/2/3/
   G/Shift+G/O/I/J/H/Shift+H/, · Ctrl+PgUp/Dn · Ctrl+=/−/0 · F3/Shift+F3 (`?`
   only when not typing). Ctrl+G opens the section picker in go-to mode,
   Ctrl+Shift+G in move-page mode; cross-section moves follow the page
   (switch to the target section with the page selected). Ctrl+H/Shift+H
   (page history / deleted pages) are inert when git isn't available.
   Ctrl+Z/Y (tree undo/redo) are global but yield to native text undo while
   typing.
3. Typing guard: inside input/textarea/contentEditable/CodeMirror, pane-local
   keys are suppressed. Exception: Tab still cycles out of the search box;
   Tab inside CodeMirror indents.
4. Pane-local keys (tree: arrows, `/`, Enter, F2, Del, Ctrl+]/[,
   Alt+↑/↓ reorder, Alt+←/→ move to section — tree moves all use Alt+arrows;
   results: arrows, Enter, `/`, `N`/`P` to step matches in the peek).

Esc peels one layer per press: modal → find panel → results→page →
editor→tree → clear tree filter. Tab cycles a view-dependent ring
(page: tree↔editor; results: tree→search→results). PgUp/PgDn scroll the main
view even when focus is elsewhere (CodeMirror keeps native paging with the
caret); in results view that means the **peek**, not the list — the list
follows the selection the arrows already drive. F3/Shift+F3 likewise route to
the peek there, since neither the editor nor the preview is mounted. The `?`
overlay is context-aware: focused-pane keys, pane keys,
globals — filterable. It is the *only* place shortcuts are listed on screen —
panes don't carry their own cheat-sheet strip. `app.helpContext` selects which
sheet it shows: the history pane binds `?` to the history-only sheet, and Esc
there steps back to history instead of closing everything.

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
