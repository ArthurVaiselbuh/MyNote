# MyNote

Local-first, keyboard-driven, OneNote-style Markdown note app. Windows is the
primary target (built for a locked-down corporate environment); macOS and Linux
are supported cross-platform builds off the same code.

> **Maintenance rule:** this file records *requirements and design decisions*.
> Update it only when a decision changes — never as part of routine code
> changes. The code is the source of truth for how things work.
>
> Up to ~3 lines per feature. Name the decision and the constraint it locks
> in; point at the file that owns it. No constants, field names, event names, or step-by-step flow —
> a reader who needs those can open the file.
>
> You may shorten existing entries of a feature you're modifying but keep it within budget,
> prioritize if your feature is more important than the existing bullet.

## Hard requirements

- **No runtime network.** No localhost server, no ports, no CDN, no external
  requests. Build-time network (npm/cargo) is fine.
- **Single small binary**, no installer. Bundling stays disabled on every
  platform (`tauri build --no-bundle`). The runtime webview is the system one:
  WebView2 (Win10/11), WKWebView (macOS), webkit2gtk on Linux — the only
  platform where the user/CI must install it (`libwebkit2gtk-4.1`).
- Notes are **plain Markdown files on disk**, AI-agent friendly: one
  `<uuid>.md` per page, first line is the `# Title` H1.

## Layout & build

- Single `src/` folder: `src/frontend` (Svelte 5 runes + Vite + CodeMirror 6),
  `src/backend` (Tauri v2 Rust crate), wired via `src/backend/tauri.conf.json`.
- All scripts live in `scripts/`: `build`/`dev`/`test` as `.ps1` (Windows) and
  `.sh` (macOS/Linux); each derives the repo root from its own location.
  Builds must go through the tauri CLI (`tauri build --no-bundle`): a plain
  `cargo build --release` yields a dev-mode binary that loads the vite devUrl
  and shows a blank window.
- Builds copy the exe to a root-level `output\` folder (`MyNote.exe`,
  `mynote-macos`, `mynote-linux`).
- **Versioning:** `scripts/version_stamp.ps1` is the only thing that edits the
  committed `major.minor` (bumps the minor across tauri.conf.json, Cargo.toml
  and Cargo.lock; local builds keep patch `9999`). CI's patch component counts
  commits since that stamp commit — `scripts/ci_version.sh` finds it by pickaxing
  the version line's history, so CI jobs need `fetch-depth: 0`.
- The config/data dir is per-platform and derived in
  `settings.rs::config_root()` — no code hardcodes `%APPDATA%`.

## On-disk format (stable contract — breaking it breaks user notebooks)

- Notebook root defaults to `<config>\MyNote\notebook`, where `<config>` is
  `%APPDATA%` / `~/Library/Application Support` / `$XDG_CONFIG_HOME`. Any
  folder can be opened via Ctrl+O.
- `notebook.json` — the only tree metadata file: section list + nested page
  tree. Page titles are cached there but the H1 in the `.md` file wins.
  `notebook.json.bak` is a crash-recovery copy `open` falls back to; all writes
  go through temp-file-plus-rename (`store::atomic_write`).
- `notebook.user.json` — volatile per-user view state, not version controlled.
- `assets/<page-id>/` — pasted images. Orphans are pruned only on notebook
  close/switch, never during a session (preserves editor undo).
- `files/<page-id>/` — arbitrary attachments, linked as plain CommonMark
  relative links; gitignored, unlike `assets/`. `trash/<page-id>/` is where
  pruning and page deletion move them instead of deleting (see below).
- **Deletes are deferred:** a delete drops the node from `notebook.json`
  immediately, but `.md` files and assets stay on disk until clean close so
  tree undo can restore them. Only ids the app deleted this session are purged
  — files it didn't delete are never touched.
- App settings (theme colors, zoom, window geometry, keybinding overrides…)
  live in `<config>\MyNote\settings.json`, not in the notebook. Window geometry
  and the MRU are backend-owned: `set_settings` ignores the frontend's copies.
- `<config>\MyNote\MyNote.log` sits alongside `settings.json`.
- `notebook.json`'s `git` key is the per-notebook for git snapshots.

## Design decisions

- **Images** are served through a custom `note-asset:` URI scheme registered in
  Rust — no asset-protocol scope juggling, no localhost server. The webview
  spells it differently per platform (`http://note-asset.localhost/` on
  Windows, `note-asset://localhost/` elsewhere); `markdown.ts` picks the base
  by userAgent. CSP `img-src` must keep both forms, and `connect-src` must keep
  `ipc: http://ipc.localhost` or Tauri IPC falls back to slow postMessage.
- **Attachments are never versioned** (`files.rs`) — living in gitignored
  `files/`/`trash/` trades away git as a safety net for no size limit and no
  repo bloat; a deleted attachment moves to `trash/` for the user to clear
  themselves, never auto-purged. They are **revealed, not launched**:
  Ctrl+Click only selects the file in the OS file manager
  (`tauri_plugin_opener::reveal_item_in_dir`), so a shared notebook can never
  use MyNote as a delivery mechanism.
- **Open/switch notebook** (Ctrl+O) is an in-app pane, not a bare OS folder
  picker: recent notebooks, plus New (refuses a folder that already has a
  `notebook.json`) and Open (native picker selecting the `notebook.json` file
  itself, so only real notebooks can be opened).
- **Backend owns all file I/O** behind Tauri commands over
  `State<Mutex<Option<Store>>>`. The frontend mutates via commands then
  re-fetches the tree — no client-side tree bookkeeping beyond display state.
- **`move_page` index contract:** the index is the position in the destination
  list counted *after* the page has been detached.
- **One notebook, one writer:** `Store::open` holds an OS-level exclusive lock
  on `.mynote.lock` for the `Store`'s lifetime.
- **Title sync is bidirectional:** editing the H1 renames the tree node, and
  tree rename (F2) rewrites the H1 in the file.
- **Search** runs in Rust over titles + bodies on demand (no persistent index),
  ranking hits by strategy; hits carry char ranges for snippet highlighting.
- **Opening a search result lands in preview**, not the editor, with the find
  prefilled from the parsed terms so every keyword lights up at once.
- **The results view peeks the selected hit** (`ResultPeek.svelte`) beside the
  list, behind its own splitter. It is inert — no `app.focus` target, no find
  panel, dead links — so the focus model and the Esc ladder are unchanged. It
  scrolls to the first highlighted term rather than the hit's line number.
- **Theme:** dark only, but colors, focus alpha, and scroll speed are
  configurable in Settings and applied as CSS variables on the app root.
- **Blank lines are WYSIWYG in the preview:** files keep the exact newlines
  typed, and `renderBody` turns runs of 2+ blank lines into nbsp spacer
  paragraphs. Deliberately nonstandard — chosen so the file stays clean plain
  Markdown instead of carrying `&nbsp;`/`<br>` tokens.
- **Import** (Ctrl+I) is one in-app pane with two sources. Both preview the
  tree that will be created — flagging duplicate titles and colliding section
  names — and always add *new* sections, never auto-merge.
  - **OneNote** (`import_mht.rs`): parses single-file `.mht` exports in Rust,
    hand-rolled MIME/quoted-printable/HTML, no new deps. One section per file,
    pages split on the export's page-separator divs, images into
    `assets/<page-id>/`. Markdown specials are not escaped (fidelity first).
  - **Markdown folder** (`import_md.rs`): copies files in as fresh `<uuid>.md`
    pages, never referencing them in place. Top-level subfolders become
    sections; deeper ones become nested parent pages, since sections can't
    nest. Titles come from each file's H1, else its filename.
- **Inline formatting beyond CommonMark uses Pandoc attribute syntax**,
  whitelisted (markdown-it stays `html:false`): `{width=420}` on images and
  `[text]{.red}` / `[text]{style="color:#hex"}` for color. Anything else
  renders literally. Chosen for on-disk consistency over Obsidian-style
  `|420`/HTML spans; Pandoc/Quarto render both forms natively.
- **External links** in the preview open in the OS browser on Ctrl/⌘+Click only
  (plain click is inert) via `tauri-plugin-opener`, whose capability is scoped
  to `http`/`https` — no mailto/tel/reveal-in-dir. The webview itself never
  navigates away: a `navigation-guard` plugin rejects main-frame navigation
  outside the app origin. The app process still makes no requests; opening a
  link is delegated to the user's browser.
- **Tree undo/redo** (Ctrl+Z/Y, a backend op stack) covers delete and move,
  not creates/renames. Ops record their inverse (detached subtree + location),
  so interleaved edits survive an undo; content relies on deferred deletion.
  While typing, Ctrl+Z/Y stay native — the tree stack owns them only outside
  text fields.
- **Logging** goes through `tauri-plugin-log` into a single size-capped
  `MyNote.log`; frontend logs share it over IPC. The level comes from
  `settings.json::logLevel` and is fixed at startup, so changing it needs a
  restart. Convention: *info* = content mutations (create/move/delete/rename,
  undo/redo, notebook open, import); *verbose* = UI events and high-frequency
  I/O (focus switches, page read/write, search). Logs are added only to
  existing code paths. **No user content in logs** — never note bodies, titles,
  query text, or filesystem paths; ids, counts, sizes and pane names only.
- **Tray icon and start-on-login** are independent opt-ins, both off by
  default. The tray turns the window's close button into hide-to-tray, and the
  tray's Quit is the only way past that guard — the guard also refuses to hide
  when the tray failed to install, so the window can never become unreachable.
  Hiding persists geometry and flushes pending edits, since the webview's own
  blur flush isn't guaranteed on `hide()`. `startOnLogin` uses
  `tauri-plugin-autostart` and registers the exe with `--hidden`, which starts
  windowless only when the tray is also enabled. Both apply without a restart.
- **The "bring MyNote to the front" chord is the one binding the OS owns** — it
  has to reach the app while another program has the keyboard, so it is
  registered system-wide and reveals the window the same way the tray's Show
  item does. Unassigned by default.
- **Autosave** debounces 3s after the last keystroke.
- **Per-notebook git snapshots** are an safety net, never a runtime
  dependency: MyNote shells out to the system `git` and silently no-ops if it
  isn't on `PATH`. Invocations null out `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM`
  so only the repo-local config MyNote writes applies — a global git-lfs filter
  or credential helper can't rewrite the stored bytes or stall a commit. Git is
  never on the data-safety path: every git failure is "skip and log".
- **History pane** (Ctrl+H revisions / Ctrl+Shift+H deleted pages) reads those
  snapshots back and is hidden entirely when git isn't available. Settings
  carries only the snapshot toggle, not an entry point — browsing is a
  keyboard/context-menu action, not a setting. It is a full-window pane rather
  than a centered modal so diffs get the whole screen, but it lives in
  `app.modal` so the keyboard-precedence rules apply unchanged; the tree and
  Editor stay mounted underneath, and the Editor *must*, or restoring has
  nothing to restore into.
  - The diff's base defaults to "now (on disk)" with the newest snapshot
    selected, so the split view reads as *what restoring this revision would
    do* (red = lines dropped, green = lines brought back). Base is movable.
    Commit shas are deliberately not shown — a revision is identified by
    timestamp and relative age.
  - All git use here is read-only, with deadlines and caps, batched through
    `cat-file --batch`, and takes its own gate so reads never block a snapshot.
    History commands drop the store lock before running git, so browsing never
    blocks autosave.
  - Diffs are computed in TypeScript (`lib/diff.ts`, no dependency) rather than
    parsed from `git diff`, so any revision can be compared with any other
    including the live buffer. The whole page is always rendered — unchanged
    runs are never collapsed.
  - Restoring replaces the CodeMirror buffer as one normal, undoable edit —
    never the on-disk file directly. Recovering a deleted page rebuilds its
    subtree from `notebook.json`'s own history (deferred deletion means the
    deleting commit often doesn't touch `notebook.json`) and counts as a
    *create*, so it is not on the tree undo stack.
- **Deferred / out of scope:** light theme, installer.

## Keyboard & focus model (the hard part — keep the precedence exact)

Four logical focus targets: `tree | editor | search | results`. Focus is
app-managed state (`app.focus`), not raw DOM focus; the focused pane gets an
outline of accent color at `--focus-alpha`.

"Ctrl+X" below means the **primary modifier**: Ctrl on Windows/Linux, Cmd (⌘)
on macOS. `keys/platform.ts` owns that platform seam.

`keys/bindings.ts` is the single source of truth for every binding: one
`COMMANDS` table the dispatcher matches against, the `?` overlay and tooltips
render from, and the keybindings pane edits. Nothing else may carry a chord
literal — adding a shortcut means adding a command there plus a case in the
matching handler.

A chord serializes as `"Mod+Alt+Shift+<key>"` in that fixed order. `chordOf`
folds the *secondary* modifier (Ctrl on macOS, Win elsewhere) into an `Other+`
prefix no binding can carry, so Ctrl+K on a Mac stays inert instead of firing
Cmd+K.

Bindings never match on `e.key` — they match on `chordKey(e)`, which is
layout-stable: ASCII letters are taken as printed (so AZERTY/QWERTZ users press
the letter they see), everything else resolves from the physical key's
US-QWERTY meaning. Without it a non-Latin layout breaks every shortcut, and
punctuation can't trust its printed character either — Hebrew puts `,` on the
Shift+`/` position, so `?` is stored as the chord `Shift+/`. Named keys
(Arrow\*, Enter, Esc, Tab, F2/F3, Delete, PageUp/Dn) pass through unchanged.

Bindings are user-configurable (Settings → Keyboard shortcuts → Customize…).
Overrides live in `settings.json::keybindings` as command id → chord list and
hold only what the user changed, so shortcuts added in a later version light up
without touching their file; an empty list means deliberately unassigned, which
is distinct from absent. Assigning a taken chord moves it off the other command
rather than creating a dead binding — `conflictOf` decides what "taken" means
from each context's reach. Esc and Tab are not rebindable: they drive the focus
ladder and dismiss every modal, so rebinding them could strand the user.

One capture-phase window keydown listener dispatches with strict precedence:

1. An open modal owns the keyboard; only Esc is handled globally (closes it; a
   stacked sub-modal steps back one layer instead).
2. Global shortcuts fire even while typing — but only chords that *can't be
   text*: `isTextChord` suppresses an unmodified letter/punctuation binding
   inside a text field, which is why `?` needs an empty editor while Ctrl+K and
   F3 do not. PgUp/PgDn count as navigation, so they still page from inside an
   input. Defaults: Ctrl+K/F/E/S/N/Shift+N/1/2/3/G/Shift+G/O/I/J/H/Shift+H/, ·
   Ctrl+PgUp/Dn · Ctrl+=/−/0 · F3/Shift+F3. Ctrl+G opens the section picker in
   go-to mode, Ctrl+Shift+G in move-page mode; cross-section moves follow the
   page. Ctrl+H/Shift+H are inert when git isn't available. Ctrl+Z/Y are global
   but yield to native text undo while typing.
3. Typing guard: inside input/textarea/contentEditable/CodeMirror, pane-local
   keys are suppressed. Exception: Tab still cycles out of the search box;
   Tab inside CodeMirror indents.
4. Pane-local keys (tree: arrows, `/`, Enter, F2, Del, Ctrl+]/[, Alt+arrows for
   all moves; results: arrows, Enter, `/`, `N`/`P` to step matches in the peek).

Esc peels one layer per press: modal → find panel → results→page →
editor→tree → clear tree filter. Tab cycles a view-dependent ring (page:
tree↔editor; results: tree→search→results). PgUp/PgDn scroll the main view even
when focus is elsewhere; in results view that means the peek, not the list,
as do F3/Shift+F3, since neither editor nor preview is mounted. The `?` overlay
is the only place shortcuts are listed on screen — panes carry no cheat-sheet
strip — and `app.helpContext` selects which sheet it shows, so the history pane
gets a history-only sheet whose Esc steps back to history.

Gotcha: the Editor unmounts in results view — anything needing the editor after
leaving results must go through app state consumed on mount (e.g.
`app.findPrefill`), not `editorCtl` calls. The `*FocusReq` counters are also
consumed on mount: their effects must stay gated on `app.focus` matching the
pane, or a remount replays a stale request and steals focus. Related:
`focusPane("tree")` must not force the page view, or the results ring could
never cycle through the tree.

## Testing

- `cargo test` in `src/backend` covers store tree ops, title sync, search,
  undo/redo, and asset/file pruning — keep it green and extend it for new
  backend logic. This is the cross-platform bar: CI runs it on all three OSes.
- E2e regression suite: Windows only — Playwright in `src/e2e` (`npm test`
  there, or `scripts\test.ps1`). It attaches to the real release exe over CDP via
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` — no
  browser downloads, no webdriver, so the no-runtime-network rule holds. CDP
  over WebView2 has no macOS/Linux equivalent, so `scripts/test.sh` runs backend +
  svelte-check only. Each test gets a scratch notebook and isolated webview
  profile, with the user's `settings.json` swapped and restored by the fixture.
  Serial only (one exe, one port), and the suite refuses to run while MyNote is
  open. Extend it for new keyboard/UI flows — keyboard races matter: wait for
  the rAF-driven title focus before sending the next key.
