//! Per-notebook git snapshots.

use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

use crate::store::GitConfig;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotKind {
    Idle,
    Close,
    Open,
}

const IDLE_GRACE_MS: u64 = 30_000;

const PROBE_DEADLINE: Duration = Duration::from_secs(3);
const INIT_DEADLINE: Duration = Duration::from_secs(10);
const CONFIG_DEADLINE: Duration = Duration::from_secs(3);
const REV_PARSE_DEADLINE: Duration = Duration::from_secs(3);
const DIFF_CACHED_DEADLINE: Duration = Duration::from_secs(10);

const HOOKS_DIR: &str = "mynote-hooks";

const GITATTRIBUTES: &str = "\
# MyNote keeps the exact bytes you typed — blank lines are WYSIWYG.
* -text
";

const GITIGNORE: &str = "\
.mynote.lock
notebook.json.bak
*.tmp

.DS_Store
Thumbs.db
desktop.ini
";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------------------------------------------------------------------
// Discovery

fn resolve(hint: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(h) = hint {
        let p = PathBuf::from(h);
        return probe(&p).then_some(p);
    }
    if probe(Path::new("git")) {
        return Some(PathBuf::from("git"));
    }
    // Git for Windows is sometimes installed but left off PATH
    #[cfg(windows)]
    {
        let candidates = [
            std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("Git/cmd/git.exe")),
            std::env::var_os("ProgramFiles(x86)")
                .map(|p| PathBuf::from(p).join("Git/cmd/git.exe")),
            std::env::var_os("LOCALAPPDATA")
                .map(|p| PathBuf::from(p).join("Programs/Git/cmd/git.exe")),
        ];
        for candidate in candidates.into_iter().flatten() {
            if probe(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn probe(candidate: &Path) -> bool {
    let mut cmd = Command::new(candidate);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_no_window(&mut cmd);
    match cmd.spawn() {
        Ok(mut child) => wait_with_deadline(&mut child, PROBE_DEADLINE)
            .map(|code| code == 0)
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn exe() -> Option<&'static PathBuf> {
    static EXE: OnceLock<Option<PathBuf>> = OnceLock::new();
    EXE.get_or_init(|| resolve(std::env::var_os("MYNOTE_GIT").as_deref()))
        .as_ref()
}

pub fn available() -> bool {
    exe().is_some()
}

pub fn is_repo(root: &Path) -> bool {
    root.join(".git").exists()
}

// ---------------------------------------------------------------------------
// Process plumbing

fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd; // no-op on non-Windows
}

/// `core.hooksPath` must point at a real, empty directory — `--no-verify`
/// skips `pre-commit`/`commit-msg` but not `post-commit`, and an empty-string
/// value makes git resolve hook names relative to cwd instead.
fn harden_args() -> Vec<String> {
    vec![
        "-c".into(),
        format!("core.hooksPath=.git/{HOOKS_DIR}"),
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        "commit.gpgSign=false".into(),
        "-c".into(),
        "tag.gpgSign=false".into(),
        "-c".into(),
        "gc.auto=0".into(),
        "-c".into(),
        "core.autocrlf=false".into(),
        "-c".into(),
        "core.safecrlf=false".into(),
        "-c".into(),
        "core.eol=lf".into(),
        "-c".into(),
        "core.askPass=".into(),
        "-c".into(),
        "credential.helper=".into(),
        "-c".into(),
        "core.quotepath=false".into(),
    ]
}

/// Only the repo-local config MyNote writes (plus `harden_args`) may apply: a
/// globally-installed `filter.lfs.*`, credential helper, alias or signing rule
/// would otherwise rewrite the bytes we store or block a snapshot.
/// `GIT_CONFIG_GLOBAL` covers `~/.gitconfig` *and* `$XDG_CONFIG_HOME/git/config`;
/// `GIT_CONFIG_NOSYSTEM` is the pre-2.32 fallback for the system file.
fn ignore_user_config(c: &mut Command) {
    #[cfg(windows)]
    const NULL_DEVICE: &str = "NUL";
    #[cfg(not(windows))]
    const NULL_DEVICE: &str = "/dev/null";

    c.env("GIT_CONFIG_GLOBAL", NULL_DEVICE)
        .env("GIT_CONFIG_SYSTEM", NULL_DEVICE)
        .env("GIT_CONFIG_NOSYSTEM", "1");
}

fn base(root: &Path) -> Command {
    let mut c = Command::new(exe().expect("caller checked available() first"));
    c.current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS");
    ignore_user_config(&mut c);
    apply_no_window(&mut c);
    c.args(harden_args());
    c
}

fn wait_with_deadline(child: &mut Child, deadline: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.code().unwrap_or(-1)),
            Ok(None) => {
                if start.elapsed() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
}

const CAPTURE_GRACE: Duration = Duration::from_millis(500);

const MAX_CAPTURE_BYTES: usize = 48 * 1024 * 1024;

fn drain_pipe<T: Read + Send + 'static>(pipe: Option<T>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    match pipe {
        // if spawn itself fails, `tx` drops with the closure and `rx` sees a
        // disconnected sender — recv_timeout(..).unwrap_or_default() below
        // treats that the same as "no output", which is the right fallback.
        Some(p) => {
            let _ = std::thread::Builder::new()
                .name("mynote-git-out".into())
                .spawn(move || {
                    let mut buf = Vec::new();
                    let _ = p.take(MAX_CAPTURE_BYTES as u64 + 1).read_to_end(&mut buf);
                    let _ = tx.send(buf);
                });
        }
        None => {
            let _ = tx.send(Vec::new());
        }
    }
    rx
}

struct RunOutput {
    code: i32,
    stderr: String,
}

/// stderr is drained on a background thread *before* waiting — a synchronous
/// read after `wait` deadlocks against git blocked writing into a full OS
/// pipe once it emits more than the pipe buffer (~64KB of warnings can do
/// it). Unlike `capture()`, the drained stderr *is* returned: callers embed
/// it in error strings that get logged, and `snapshot()`'s identity retry
/// matches on its text.
fn run(root: &Path, args: &[&str], deadline: Duration) -> Result<RunOutput, String> {
    let mut child = base(root).args(args).spawn().map_err(err)?;
    let err_rx = drain_pipe(child.stderr.take());
    let code = match wait_with_deadline(&mut child, deadline) {
        Some(code) => code,
        None => return Err("git timed out".to_string()),
    };
    let stderr =
        String::from_utf8_lossy(&err_rx.recv_timeout(CAPTURE_GRACE).unwrap_or_default()).into_owned();
    Ok(RunOutput { code, stderr })
}

// ---------------------------------------------------------------------------
// Repo setup

fn seed_file_if_absent(path: &Path, contents: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, contents).map_err(err)
}

fn run_config(root: &Path, key: &str, value: &str) -> Result<(), String> {
    let out = run(root, &["config", "--local", key, value], CONFIG_DEADLINE)?;
    if out.code != 0 {
        return Err(format!("git config {key} failed: {}", out.stderr.trim()));
    }
    Ok(())
}

fn has_identity(root: &Path) -> bool {
    run(root, &["config", "--get", "user.email"], CONFIG_DEADLINE)
        .map(|o| o.code == 0)
        .unwrap_or(false)
}

fn ensure_identity(root: &Path) -> Result<(), String> {
    if has_identity(root) {
        return Ok(());
    }
    run_config(root, "user.name", "MyNote")?;
    run_config(root, "user.email", "mynote@localhost")?;
    Ok(())
}

/// Idempotent. Seeds `.gitattributes`/`.gitignore` before the first `git add`
/// so they govern the first commit too — git reads attributes from the
/// working tree at add time.
pub fn ensure_repo(root: &Path) -> Result<(), String> {
    if !available() {
        return Err("git not found on PATH".to_string());
    }
    if !is_repo(root) {
        let out = run(
            root,
            &[
                "-c",
                "init.templateDir=",
                "-c",
                "init.defaultBranch=main",
                "init",
                "--quiet",
            ],
            INIT_DEADLINE,
        )?;
        if out.code != 0 {
            return Err(format!("git init failed: {}", out.stderr.trim()));
        }
    }
    fs::create_dir_all(root.join(".git").join(HOOKS_DIR)).map_err(err)?;
    seed_file_if_absent(&root.join(".gitattributes"), GITATTRIBUTES)?;
    seed_file_if_absent(&root.join(".gitignore"), GITIGNORE)?;
    run_config(root, "core.autocrlf", "false")?;
    run_config(root, "core.safecrlf", "false")?;
    run_config(root, "commit.gpgsign", "false")?;
    run_config(root, "core.hooksPath", &format!(".git/{HOOKS_DIR}"))?;
    ensure_identity(root)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshotting

fn has_head(root: &Path) -> bool {
    run(
        root,
        &["rev-parse", "--verify", "--quiet", "HEAD"],
        REV_PARSE_DEADLINE,
    )
    .map(|o| o.code == 0)
    .unwrap_or(false)
}

fn has_staged_changes(root: &Path) -> Result<bool, String> {
    let out = run(root, &["diff", "--cached", "--quiet"], DIFF_CACHED_DEADLINE)?;
    match out.code {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!("git diff --cached failed: {}", out.stderr.trim())),
    }
}

fn commit_message() -> String {
    format!("MyNote snapshot {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
}

fn commit(root: &Path, deadline: Duration) -> Result<RunOutput, String> {
    let msg = commit_message();
    run(
        root,
        &["commit", "--quiet", "--no-verify", "--no-gpg-sign", "-m", &msg],
        deadline,
    )
}

/// Stages and commits everything in `root`. `Ok(true)` if a commit was made,
/// `Ok(false)` if there was nothing to commit.
pub fn snapshot(root: &Path, kind: SnapshotKind, deadline: Duration) -> Result<bool, String> {
    if !available() {
        return Err("git not found on PATH".to_string());
    }
    if !is_repo(root) {
        return Err("not a git repo".to_string());
    }
    let add_out = run(root, &["add", "-A"], deadline)?;
    if add_out.code != 0 {
        return Err(format!("git add failed: {}", add_out.stderr.trim()));
    }
    if has_head(root) && !has_staged_changes(root)? {
        return Ok(false);
    }
    let out = commit(root, deadline)?;
    if out.code == 0 {
        log::info!("git snapshot committed ({kind:?})");
        return Ok(true);
    }
    let stderr_lc = out.stderr.to_lowercase();
    if stderr_lc.contains("tell me who you are") || stderr_lc.contains("empty ident") {
        ensure_identity(root)?;
        let retry = commit(root, deadline)?;
        if retry.code == 0 {
            log::info!("git snapshot committed ({kind:?})");
            return Ok(true);
        }
        return Err(format!("git commit failed: {}", retry.stderr.trim()));
    }
    Err(format!("git commit failed: {}", out.stderr.trim()))
}

pub fn should_snapshot(cfg: &GitConfig, seq: u64, committed: u64, idle_ms: u64, since_commit_ms: u64) -> bool {
    cfg.enabled
        && seq != committed
        && idle_ms >= IDLE_GRACE_MS
        && since_commit_ms >= cfg.interval_secs.saturating_mul(1000)
}

// ---------------------------------------------------------------------------
// Read-only history queries
//
// `log` / `cat-file` / `ls-tree` never touch the index or working tree, so —
// unlike ensure_repo/snapshot — these take no gate and are safe to run while
// a snapshot is in flight (`GIT_OPTIONAL_LOCKS=0` in `base()` already stops
// opportunistic index refreshes).

const LOG_DEADLINE: Duration = Duration::from_secs(10);
const CAT_FILE_DEADLINE: Duration = Duration::from_secs(10);
const BATCH_DEADLINE: Duration = Duration::from_secs(20);

const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_BATCH_SPECS: usize = 512;

pub const MAX_PAGE_REVISIONS: usize = 300;
pub const MAX_DELETE_COMMITS: usize = 200;

const RECORD_SEP: char = '\u{1e}';
const FIELD_SEP: char = '\u{1f}';

fn guard(root: &Path) -> Result<(), String> {
    if !available() {
        return Err("git not found on PATH".to_string());
    }
    if !is_repo(root) {
        return Err("not a git repo".to_string());
    }
    Ok(())
}

/// A bare commit sha (7-64 hex chars, so both full and abbreviated forms
/// work) — the only revision shape callers outside this module should ever
/// hand in. Exported so `commands.rs`/`history.rs` can validate a raw `sha`
/// argument from the frontend before it reaches any git subprocess.
pub fn is_commit_sha(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `is_commit_sha`, plus an optional `~N` parent-walk suffix — the only
/// revision shapes this module ever builds internally (e.g. `"{sha}~1"`).
/// Rejects anything that could be parsed as a git option (a leading `-`)
/// before it reaches argv.
fn is_safe_rev(rev: &str) -> bool {
    let (base, suffix) = match rev.split_once('~') {
        Some((b, s)) => (b, Some(s)),
        None => (rev, None),
    };
    let suffix_ok = suffix.map_or(true, |s| !s.is_empty() && s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()));
    is_commit_sha(base) && suffix_ok
}

struct Captured {
    code: i32,
    stdout: Vec<u8>,
}

/// `base()` nulls stdout, so anything whose output we actually need goes
/// through here. Both pipes are drained on background threads rather than
/// read synchronously after `wait`: a blocking read on this thread could
/// deadlock against git blocked writing into a full OS pipe once output
/// exceeds a few dozen KB (a blob capture routinely does). Uses the existing
/// `wait_with_deadline`, so a hang still gets killed on schedule.
fn capture(
    root: &Path,
    args: &[&str],
    stdin_data: Option<&[u8]>,
    deadline: Duration,
) -> Result<Captured, String> {
    let mut cmd = base(root);
    cmd.args(args).stdout(Stdio::piped());
    if stdin_data.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().map_err(err)?;

    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            let data = data.to_vec();
            let _ = std::thread::Builder::new()
                .name("mynote-git-in".into())
                .spawn(move || {
                    let _ = stdin.write_all(&data);
                    // stdin drops here, closing the pipe so git sees EOF
                });
        }
    }

    let out_rx = drain_pipe(child.stdout.take());
    let err_rx = drain_pipe(child.stderr.take());

    let code = match wait_with_deadline(&mut child, deadline) {
        Some(code) => code,
        None => return Err("git timed out".to_string()),
    };
    let stdout = out_rx.recv_timeout(CAPTURE_GRACE).unwrap_or_default();
    let _ = err_rx.recv_timeout(CAPTURE_GRACE); // drained, never logged (may embed notebook paths)
    Ok(Captured { code, stdout })
}

pub struct Revision {
    pub sha: String,
    pub at: i64,
    pub subject: String,
}

fn parse_log_records(bytes: &[u8]) -> Vec<Revision> {
    String::from_utf8_lossy(bytes)
        .split(RECORD_SEP)
        .filter(|s| !s.is_empty())
        .filter_map(|rec| {
            let rec = rec.strip_suffix('\n').unwrap_or(rec);
            let mut parts = rec.splitn(3, FIELD_SEP);
            let sha = parts.next()?.to_string();
            let at: i64 = parts.next()?.parse().ok()?;
            let subject = parts.next().unwrap_or("").to_string();
            Some(Revision { sha, at, subject })
        })
        .collect()
}

/// Revisions of one path, newest first. `rel` is a repo-relative path with no
/// pathspec magic characters (page ids and `notebook.json` both qualify).
/// A repo with no commits, or a path never committed, is `Ok(vec![])` — git
/// exits 128 with empty stdout for the former, which is otherwise
/// indistinguishable from "not a git error" without checking stdout too.
pub fn log_file(root: &Path, rel: &str, limit: usize) -> Result<Vec<Revision>, String> {
    guard(root)?;
    let limit = limit.clamp(1, MAX_PAGE_REVISIONS);
    let limit_arg = format!("--max-count={limit}");
    let fmt_arg = format!("--format={RECORD_SEP}%H{FIELD_SEP}%at{FIELD_SEP}%s");
    let out = capture(
        root,
        &["log", "--no-color", "--no-decorate", "--no-renames", &limit_arg, &fmt_arg, "--", rel],
        None,
        LOG_DEADLINE,
    )?;
    if out.code != 0 {
        if out.stdout.is_empty() {
            return Ok(Vec::new());
        }
        return Err(format!("git log exited with {}", out.code));
    }
    Ok(parse_log_records(&out.stdout))
}

/// The author timestamp (epoch seconds) of one commit. Used to locate where
/// a deleting commit falls in `notebook.json`'s own revision list, since the
/// deleting commit itself frequently doesn't touch `notebook.json` at all
/// (deferred deletion: the tree node was already dropped in an earlier idle
/// snapshot, so only the file removal lands in the commit being probed).
pub fn commit_time(root: &Path, sha: &str) -> Result<i64, String> {
    guard(root)?;
    if !is_commit_sha(sha) {
        return Err("invalid revision".to_string());
    }
    let out = capture(root, &["log", "-1", "--format=%at", sha], None, LOG_DEADLINE)?;
    if out.code != 0 {
        return Err(format!("git log exited with {}", out.code));
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<i64>().map_err(|_| "malformed commit time".to_string())
}

pub struct BlobText {
    pub text: String,
    pub truncated: bool,
    pub missing: bool,
}

fn blob_bytes_to_text(bytes: Vec<u8>) -> Result<BlobText, String> {
    let probe_len = bytes.len().min(8192);
    if bytes[..probe_len].contains(&0) {
        return Err("that revision isn't text".to_string());
    }
    if bytes.len() <= MAX_TEXT_BYTES {
        return String::from_utf8(bytes)
            .map(|text| BlobText { text, truncated: false, missing: false })
            .map_err(|_| "that revision isn't valid text".to_string());
    }
    let slice = &bytes[..MAX_TEXT_BYTES];
    let valid_up_to = match std::str::from_utf8(slice) {
        Ok(s) => s.len(),
        Err(e) => e.valid_up_to(),
    };
    let text = String::from_utf8_lossy(&slice[..valid_up_to]).into_owned();
    Ok(BlobText { text, truncated: true, missing: false })
}

/// The text of one blob at one revision. `rev` is a bare sha (optionally with
/// a `~N` suffix); `rel` is a repo-relative path with no pathspec magic.
pub fn read_blob_text(root: &Path, rev: &str, rel: &str) -> Result<BlobText, String> {
    guard(root)?;
    if !is_safe_rev(rev) {
        return Err("invalid revision".to_string());
    }
    let spec = format!("{rev}:{rel}");
    let out = capture(root, &["cat-file", "blob", &spec], None, CAT_FILE_DEADLINE)?;
    if out.code != 0 {
        return Ok(BlobText { text: String::new(), truncated: false, missing: true });
    }
    blob_bytes_to_text(out.stdout)
}

fn parse_batch_output(bytes: &[u8], expected: usize) -> Vec<Option<Vec<u8>>> {
    let mut out = Vec::with_capacity(expected);
    let mut pos = 0usize;
    while out.len() < expected && pos < bytes.len() {
        let Some(nl) = bytes[pos..].iter().position(|&b| b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&bytes[pos..pos + nl]).into_owned();
        pos += nl + 1;
        if header.ends_with(" missing") {
            out.push(None);
            continue;
        }
        // "<oid> <type> <size>" — size is the only field we need.
        let Some(size) = header.rsplit(' ').next().and_then(|s| s.parse::<usize>().ok()) else {
            break; // malformed header — bail rather than misparse the rest of the stream
        };
        if pos + size > bytes.len() {
            break; // stream truncated (deadline hit / MAX_CAPTURE_BYTES cap)
        }
        out.push(Some(bytes[pos..pos + size].to_vec()));
        pos += size;
        if pos < bytes.len() && bytes[pos] == b'\n' {
            pos += 1; // trailing separator after the blob content
        }
    }
    while out.len() < expected {
        out.push(None);
    }
    out
}

/// Bulk blob fetch: one process regardless of how many `specs` (`"<rev>:<rel>"`)
/// are requested, chunked at `MAX_BATCH_SPECS`. Results are positional — index
/// `i` of the result is the blob for `specs[i]`, `None` when that path didn't
/// exist at that revision (or the response stream was truncated).
pub fn read_blobs(root: &Path, specs: &[String]) -> Result<Vec<Option<Vec<u8>>>, String> {
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    guard(root)?;
    let mut results = Vec::with_capacity(specs.len());
    for chunk in specs.chunks(MAX_BATCH_SPECS) {
        let mut input = String::new();
        for s in chunk {
            input.push_str(s);
            input.push('\n');
        }
        let out = capture(root, &["cat-file", "--batch"], Some(input.as_bytes()), BATCH_DEADLINE)?;
        if out.code != 0 {
            return Err(format!("git cat-file exited with {}", out.code));
        }
        results.extend(parse_batch_output(&out.stdout, chunk.len()));
    }
    Ok(results)
}

pub struct DeletedGroup {
    pub sha: String,
    pub at: i64,
    pub ids: Vec<String>,
}

fn page_id_from_deleted_path(path: &str) -> Option<String> {
    let name = path.strip_suffix(".md")?;
    if name.contains('/') {
        return None; // e.g. assets/<id>/foo.png deleted alongside the page
    }
    crate::store::is_page_id(name).then(|| name.to_string())
}

fn parse_deleted_groups(bytes: &[u8]) -> Vec<DeletedGroup> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for rec in text.split(RECORD_SEP) {
        if rec.is_empty() {
            continue;
        }
        // header and the NUL-separated path list are joined by a literal
        // '\n' (the pretty-format's own terminator, printed regardless of
        // -z; -z only affects the record separator and the path list).
        let mut fields = rec.splitn(2, '\u{0}');
        let Some(header) = fields.next() else { continue };
        let rest = fields.next().unwrap_or("").strip_prefix('\n').unwrap_or("");
        let mut hp = header.splitn(2, FIELD_SEP);
        let Some(sha) = hp.next() else { continue };
        let Some(at) = hp.next().and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let ids: Vec<String> = rest
            .split('\u{0}')
            .filter(|p| !p.is_empty())
            .filter_map(page_id_from_deleted_path)
            .collect();
        if !ids.is_empty() {
            out.push(DeletedGroup { sha: sha.to_string(), at, ids });
        }
    }
    out
}

/// Commits that deleted at least one `<uuid>.md` page file, newest first,
/// each paired with the ids it removed (a deferred-delete close commit removes
/// a whole subtree at once, which is exactly the grouping callers want).
/// `--diff-filter=D` + `--name-only` restricts the name list to deletions
/// only; `--no-renames` stops a delete from being paired away as a rename of
/// an unrelated new page.
///
/// This is raw per-commit history: an id that was deleted, recreated, and
/// deleted again appears in two groups. Cross-commit dedup and filtering
/// against the live tree (an id that's back may not be "deleted" anymore)
/// is a business-logic concern for the caller, who has the store to check
/// against — not something this thin wrapper can decide on its own.
pub fn deleted_md_groups(root: &Path, limit: usize) -> Result<Vec<DeletedGroup>, String> {
    guard(root)?;
    let limit = limit.clamp(1, MAX_DELETE_COMMITS);
    let limit_arg = format!("--max-count={limit}");
    let fmt_arg = format!("--format={RECORD_SEP}%H{FIELD_SEP}%at");
    let out = capture(
        root,
        &[
            "log", "--no-color", "--no-decorate", "--diff-filter=D", "--no-renames",
            "--name-only", "-z", &fmt_arg, &limit_arg,
        ],
        None,
        LOG_DEADLINE,
    )?;
    if out.code != 0 {
        if out.stdout.is_empty() {
            return Ok(Vec::new());
        }
        return Err(format!("git log exited with {}", out.code));
    }
    Ok(parse_deleted_groups(&out.stdout))
}

/// Repo-relative paths under `dir_prefix` at `rev`. Empty (not an error) when
/// the directory doesn't exist at that revision.
pub fn list_tree_files(root: &Path, rev: &str, dir_prefix: &str) -> Result<Vec<String>, String> {
    guard(root)?;
    if !is_safe_rev(rev) {
        return Err("invalid revision".to_string());
    }
    let out = capture(root, &["ls-tree", "-r", "-z", "--name-only", rev, "--", dir_prefix], None, LOG_DEADLINE)?;
    if out.code != 0 {
        return Err(format!("git ls-tree exited with {}", out.code));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\u{0}')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(enabled: bool, interval_secs: u64) -> GitConfig {
        GitConfig {
            enabled,
            interval_secs,
        }
    }

    #[test]
    fn resolve_returns_none_for_bogus_exe() {
        let hint = OsStr::new("Z:/no/such/git-binary-here.exe");
        assert!(resolve(Some(hint)).is_none());
    }

    #[test]
    fn ignore_and_attributes_content() {
        assert!(GITIGNORE.contains(".mynote.lock"));
        assert!(GITIGNORE.contains("notebook.json.bak"));
        assert!(GITIGNORE.contains("*.tmp"));
        assert!(GITATTRIBUTES.contains("* -text"));
    }

    #[test]
    fn should_snapshot_matrix() {
        assert!(!should_snapshot(&cfg(false, 3600), 5, 0, 1_000_000, 1_000_000));
        assert!(!should_snapshot(&cfg(true, 3600), 5, 5, 1_000_000, 1_000_000));
        assert!(!should_snapshot(&cfg(true, 3600), 5, 0, 1_000, 3_600_000));
        assert!(!should_snapshot(&cfg(true, 3600), 5, 0, 60_000, 10_000));
        assert!(should_snapshot(&cfg(true, 3600), 5, 0, 60_000, 3_600_001));
        assert!(!should_snapshot(&cfg(true, 0), 5, 0, 1_000, 1_000_000));
        assert!(should_snapshot(&cfg(true, 0), 5, 0, 30_000, 0));
    }

    macro_rules! need_git {
        () => {
            if !available() {
                eprintln!("skipping: git not found on PATH");
                return;
            }
        };
    }

    #[test]
    fn git_is_available_on_ci() {
        if std::env::var("CI").is_ok() {
            assert!(available(), "CI runners are expected to ship git");
        }
    }

    fn init_temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        ensure_repo(dir.path()).unwrap();
        dir
    }

    #[test]
    fn enable_creates_repo_and_first_commit() {
        need_git!();
        let dir = init_temp_repo();
        fs::write(dir.path().join("notebook.json"), "{}").unwrap();
        fs::write(dir.path().join("AGENTS.md"), "hi").unwrap();
        let uuid_md = format!("{}.md", crate::store::new_id());
        fs::write(dir.path().join(&uuid_md), "# Page\n").unwrap();
        fs::write(dir.path().join(".mynote.lock"), "").unwrap();
        fs::write(dir.path().join("notebook.json.bak"), "{}").unwrap();
        fs::write(dir.path().join("scratch.tmp"), "junk").unwrap();

        assert!(dir.path().join(".git").is_dir());
        assert!(dir.path().join(".git").join(HOOKS_DIR).is_dir());

        let committed = snapshot(dir.path(), SnapshotKind::Open, Duration::from_secs(20)).unwrap();
        assert!(committed);

        let out = run(dir.path(), &["log", "--oneline"], Duration::from_secs(5)).unwrap();
        assert_eq!(out.code, 0);

        let files_out = base(dir.path())
            .args(["ls-files"])
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&files_out.stdout);
        assert!(files.contains("notebook.json"));
        assert!(files.contains("AGENTS.md"));
        assert!(files.contains(&uuid_md));
        assert!(!files.contains(".mynote.lock"));
        assert!(!files.contains("notebook.json.bak"));
        assert!(!files.contains("scratch.tmp"));
    }

    #[test]
    fn snapshot_is_a_noop_when_nothing_changed() {
        need_git!();
        let dir = init_temp_repo();
        fs::write(dir.path().join("notebook.json"), "{}").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        assert!(!snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let files_out = base(dir.path())
            .args(["log", "--oneline"])
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&files_out.stdout);
        assert_eq!(log.lines().count(), 1);
    }

    #[test]
    fn snapshot_preserves_exact_bytes_including_crlf() {
        need_git!();
        let dir = init_temp_repo();
        run_config(dir.path(), "core.autocrlf", "true").unwrap();

        let content = "# T\r\n\r\n\r\na\r\n";
        fs::write(dir.path().join("page.md"), content).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let blob = base(dir.path())
            .args(["cat-file", "blob", "HEAD:page.md"])
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(blob.stdout, content.as_bytes());

        let clean = run(dir.path(), &["diff", "--quiet"], Duration::from_secs(5)).unwrap();
        assert_eq!(clean.code, 0, "autocrlf left the tree dirty — -text isn't working");
    }

    #[test]
    fn snapshot_succeeds_with_no_user_identity() {
        need_git!();
        let dir = init_temp_repo();
        run(
            dir.path(),
            &["config", "--local", "--unset", "user.email"],
            Duration::from_secs(3),
        )
        .unwrap();
        run(
            dir.path(),
            &["config", "--local", "--unset", "user.name"],
            Duration::from_secs(3),
        )
        .unwrap();
        fs::write(dir.path().join("notebook.json"), "{\"x\":1}").unwrap();

        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
    }

    /// Points a git process at a throwaway home directory holding a global
    /// config, without touching this process's own environment (tests run in
    /// parallel). Applied *after* `base()` so it can only add the hostile
    /// home, never undo the `GIT_CONFIG_GLOBAL` hardening under test.
    fn fake_global_config_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        fs::write(
            home.path().join(".gitconfig"),
            "[user]\n\temail = global@example.com\n[filter \"lfs\"]\n\tclean = exit 1\n",
        )
        .unwrap();
        home
    }

    fn init_bare_repo_without_identity() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let out = run(
            dir.path(),
            &["-c", "init.templateDir=", "-c", "init.defaultBranch=main", "init", "--quiet"],
            INIT_DEADLINE,
        )
        .unwrap();
        assert_eq!(out.code, 0);
        dir
    }

    fn read_user_email(mut cmd: Command, home: &Path) -> String {
        let out = cmd
            .args(["config", "--get", "user.email"])
            .env("HOME", home)
            .env("USERPROFILE", home)
            .env_remove("XDG_CONFIG_HOME")
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn global_config_is_never_read() {
        need_git!();
        let dir = init_bare_repo_without_identity();
        let home = fake_global_config_home();

        let mut unhardened = Command::new(exe().unwrap());
        unhardened.current_dir(dir.path());
        assert_eq!(
            read_user_email(unhardened, home.path()),
            "global@example.com",
            "fixture is broken — git isn't picking up the fake global config at all"
        );

        assert_eq!(
            read_user_email(base(dir.path()), home.path()),
            "",
            "MyNote's git must not see the user's global config"
        );
    }

    #[test]
    fn snapshot_records_pruned_assets() {
        need_git!();
        let dir = tempfile::tempdir().unwrap();
        let mut store = crate::store::Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        let img = crate::assets::save_image(
            &store,
            &page.id,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            "png",
        )
        .unwrap();
        store.write_page(&page.id, "# Pics\n\nno image here\n").unwrap();
        ensure_repo(dir.path()).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Open, Duration::from_secs(20)).unwrap());

        let info = store.close();
        assert!(snapshot(&info.root, SnapshotKind::Close, Duration::from_secs(5)).unwrap());

        let files_out = base(dir.path())
            .args(["ls-files"])
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&files_out.stdout);
        assert!(!files.contains(&img), "pruned asset should not be tracked after the close commit");
    }

    /// Would hang (and then "git timed out") without the concurrent stderr
    /// drain in `run()`: git blocks writing warnings into the full OS pipe
    /// and never exits. Uses a repo without `ensure_repo`'s `* -text`
    /// .gitattributes, so `core.autocrlf=true` emits a CRLF warning per file
    /// — 800 files with long names pushes stderr well past the ~64KB pipe
    /// buffer.
    #[test]
    fn run_survives_stderr_larger_than_the_pipe_buffer() {
        need_git!();
        let dir = init_bare_repo_without_identity();
        let pad = "x".repeat(60);
        for i in 0..800 {
            fs::write(dir.path().join(format!("f-{pad}-{i}.txt")), "a\nb\nc\n").unwrap();
        }
        let out = run(
            dir.path(),
            &["-c", "core.autocrlf=true", "-c", "core.safecrlf=warn", "add", "-A"],
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(out.code, 0);
        assert!(
            out.stderr.len() > 64 * 1024,
            "stderr was only {} bytes — not past the pipe buffer",
            out.stderr.len()
        );
        assert!(out.stderr.contains("LF will be replaced"));
    }

    #[test]
    fn hooks_are_not_executed() {
        need_git!();
        let dir = init_temp_repo();
        let hooks = dir.path().join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let marker = dir.path().join("hook-ran");
        for name in ["pre-commit", "post-commit"] {
            #[cfg(windows)]
            let script = format!("echo ran > \"{}\"\n", marker.display());
            #[cfg(not(windows))]
            let script = format!("#!/bin/sh\necho ran > \"{}\"\n", marker.display());
            let path = hooks.join(name);
            fs::write(&path, script).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&path, perms).unwrap();
            }
        }
        fs::write(dir.path().join("notebook.json"), "{}").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        assert!(!marker.exists(), "a global-style hook must never run for a MyNote snapshot");
    }

    #[test]
    fn ensure_repo_is_idempotent_and_never_clobbers() {
        need_git!();
        let dir = init_temp_repo();
        fs::write(dir.path().join(".gitignore"), "custom-edit\n").unwrap();
        ensure_repo(dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "custom-edit\n"
        );
    }

    // -----------------------------------------------------------------
    // History queries

    #[test]
    fn is_page_id_and_is_safe_rev_reject_junk() {
        assert!(is_safe_rev("abc1234"));
        assert!(is_safe_rev("abc1234~1"));
        assert!(is_safe_rev("abc1234~12"));
        assert!(!is_safe_rev("-oops"));
        assert!(!is_safe_rev("abc1234~"));
        assert!(!is_safe_rev("abc1234~123"));
        assert!(!is_safe_rev("not-hex-at-all"));
        assert!(!is_safe_rev(""));

        assert!(is_commit_sha("abc1234"));
        assert!(!is_commit_sha("abc1234~1"));
        assert!(!is_commit_sha("-oops"));
    }

    #[test]
    fn commit_time_matches_the_log_and_rejects_unsafe_input() {
        need_git!();
        let dir = init_temp_repo();
        fs::write(dir.path().join("notebook.json"), "{}").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        let sha = head_sha(dir.path());
        let revs = log_file(dir.path(), "notebook.json", 10).unwrap();
        assert_eq!(commit_time(dir.path(), &sha).unwrap(), revs[0].at);
        assert!(commit_time(dir.path(), "--upload-pack=x").is_err());
    }

    #[test]
    fn parse_batch_output_maps_positionally_including_binary_and_missing() {
        let mut stream = Vec::new();
        stream.extend_from_slice(b"aaaa blob 5\nhello\n");
        stream.extend_from_slice(b"HEAD:nope.md missing\n");
        stream.extend_from_slice(b"bbbb blob 4\n\x00\x01\x02\x03\n");
        let out = parse_batch_output(&stream, 3);
        assert_eq!(out[0], Some(b"hello".to_vec()));
        assert_eq!(out[1], None);
        assert_eq!(out[2], Some(vec![0, 1, 2, 3]));
    }

    #[test]
    fn parse_batch_output_pads_missing_tail_on_truncated_stream() {
        let out = parse_batch_output(b"aaaa blob 5\nhello\n", 3);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], Some(b"hello".to_vec()));
        assert_eq!(out[1], None);
        assert_eq!(out[2], None);
    }

    fn commit_uuid_page(dir: &Path, content: &str) -> String {
        let id = crate::store::new_id();
        fs::write(dir.join(format!("{id}.md")), content).unwrap();
        assert!(snapshot(dir, SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        id
    }

    /// Real callers only ever pass a bare sha (from `log_file`/`deleted_md_groups`),
    /// never the ref name "HEAD" — `is_safe_rev` rejects that on purpose, so
    /// tests resolve it themselves instead of loosening the validator.
    fn head_sha(dir: &Path) -> String {
        let out = capture(dir, &["rev-parse", "HEAD"], None, Duration::from_secs(5)).unwrap();
        assert_eq!(out.code, 0);
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn log_file_lists_newest_first_and_respects_the_cap() {
        need_git!();
        let dir = init_temp_repo();
        let id = commit_uuid_page(dir.path(), "# T\n\nv1\n");
        fs::write(dir.path().join(format!("{id}.md")), "# T\n\nv2\n").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let revs = log_file(dir.path(), &format!("{id}.md"), 10).unwrap();
        assert_eq!(revs.len(), 2);
        assert!(revs[0].at >= revs[1].at);
        assert_ne!(revs[0].sha, revs[1].sha);
        assert!(revs[0].subject.starts_with("MyNote snapshot"));

        let capped = log_file(dir.path(), &format!("{id}.md"), 1).unwrap();
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0].sha, revs[0].sha);
    }

    #[test]
    fn log_file_is_empty_for_headless_repo_and_untracked_path() {
        need_git!();
        let dir = init_temp_repo();
        // no commits at all yet
        assert!(log_file(dir.path(), "notebook.json", 10).unwrap().is_empty());
        fs::write(dir.path().join("notebook.json"), "{}").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        // a path that was never committed
        assert!(log_file(dir.path(), "never-existed.md", 10).unwrap().is_empty());
    }


    #[test]
    fn read_blob_text_preserves_exact_bytes_including_crlf() {
        need_git!();
        let dir = init_temp_repo();
        run_config(dir.path(), "core.autocrlf", "true").unwrap();
        let content = "# T\r\n\r\n\r\na\r\n";
        fs::write(dir.path().join("page.md"), content).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let sha = head_sha(dir.path());
        let blob = read_blob_text(dir.path(), &sha, "page.md").unwrap();
        assert_eq!(blob.text, content);
        assert!(!blob.truncated);
    }

    #[test]
    fn read_blob_text_rejects_binary_but_reports_a_missing_path_as_ok() {
        need_git!();
        let dir = init_temp_repo();
        fs::write(dir.path().join("img.png"), [0u8, 1, 2, 137, 80, 78, 71]).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        let sha = head_sha(dir.path());
        assert!(read_blob_text(dir.path(), &sha, "img.png").is_err());
        let missing = read_blob_text(dir.path(), &sha, "nope.md").unwrap();
        assert!(missing.missing);
        assert_eq!(missing.text, "");
    }

    #[test]
    fn log_file_includes_the_delete_commit_and_it_reads_back_as_missing() {
        need_git!();
        let dir = init_temp_repo();
        let id = commit_uuid_page(dir.path(), "# T\n\nv1\n");
        fs::remove_file(dir.path().join(format!("{id}.md"))).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        fs::write(dir.path().join(format!("{id}.md")), "# T\n\nv2\n").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let revs = log_file(dir.path(), &format!("{id}.md"), 10).unwrap();
        assert_eq!(revs.len(), 3, "recreate, delete, and create all remain viewable revisions");

        let texts: Vec<BlobText> = revs
            .iter()
            .map(|r| read_blob_text(dir.path(), &r.sha, &format!("{id}.md")).unwrap())
            .collect();
        assert_eq!(texts[0].text, "# T\n\nv2\n");
        assert!(!texts[0].missing);
        assert!(texts[1].missing, "the delete commit itself has no file — that's expected, not an error");
        assert_eq!(texts[2].text, "# T\n\nv1\n");
        assert!(!texts[2].missing);
    }

    #[test]
    fn read_blob_text_rejects_unsafe_revisions() {
        need_git!();
        let dir = init_temp_repo();
        fs::write(dir.path().join("a.md"), "hi").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        assert!(read_blob_text(dir.path(), "--upload-pack=x", "a.md").is_err());
    }

    #[test]
    fn read_blobs_batch_round_trips_against_a_real_repo() {
        need_git!();
        let dir = init_temp_repo();
        let id = commit_uuid_page(dir.path(), "# T\n\nhello\n");
        let specs = vec![
            format!("HEAD:{id}.md"),
            "HEAD:does-not-exist.md".to_string(),
        ];
        let out = read_blobs(dir.path(), &specs).unwrap();
        assert_eq!(out[0].as_deref(), Some("# T\n\nhello\n".as_bytes()));
        assert_eq!(out[1], None);
    }

    #[test]
    fn deleted_md_groups_reports_the_removing_commit_and_ignores_other_files() {
        need_git!();
        let dir = init_temp_repo();
        let id = commit_uuid_page(dir.path(), "# T\n\nbody\n");
        fs::write(dir.path().join("AGENTS.md"), "hi").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        fs::remove_file(dir.path().join(format!("{id}.md"))).unwrap();
        fs::remove_file(dir.path().join("AGENTS.md")).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let groups = deleted_md_groups(dir.path(), 50).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].ids, vec![id]);
    }

    #[test]
    fn deleted_md_groups_reports_every_delete_event_raw() {
        // dedup/"is it still gone" filtering happens one layer up, against
        // live tree state — this thin wrapper just mirrors git history as-is.
        need_git!();
        let dir = init_temp_repo();
        let id = commit_uuid_page(dir.path(), "# T\n\nv1\n");
        fs::remove_file(dir.path().join(format!("{id}.md"))).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        // recreate and delete again
        fs::write(dir.path().join(format!("{id}.md")), "# T\n\nv2\n").unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());
        fs::remove_file(dir.path().join(format!("{id}.md"))).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let groups = deleted_md_groups(dir.path(), 50).unwrap();
        let total: usize = groups.iter().map(|g| g.ids.len()).sum();
        assert_eq!(total, 2, "both delete events for this id should be reported");
        assert!(groups[0].at >= groups[1].at, "newest first");
    }

    #[test]
    fn list_tree_files_lists_assets_at_the_parent_commit() {
        need_git!();
        let dir = init_temp_repo();
        let id = crate::store::new_id();
        fs::create_dir_all(dir.path().join("assets").join(&id)).unwrap();
        fs::write(dir.path().join("assets").join(&id).join("img.png"), [1, 2, 3]).unwrap();
        assert!(snapshot(dir.path(), SnapshotKind::Idle, Duration::from_secs(20)).unwrap());

        let sha = head_sha(dir.path());
        let files = list_tree_files(dir.path(), &sha, &format!("assets/{id}")).unwrap();
        assert_eq!(files, vec![format!("assets/{id}/img.png")]);

        // a directory that never existed at this rev is empty, not an error
        let missing = list_tree_files(dir.path(), &sha, "assets/does-not-exist").unwrap();
        assert!(missing.is_empty());
    }
}
