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
- `notebook.json` is the only tree metadata file; `notebook.json.bak` is a
  crash-recovery copy `open` falls back to. All file writes go through
  temp-file-plus-rename (`store::atomic_write`) so a crash can't truncate them.
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
