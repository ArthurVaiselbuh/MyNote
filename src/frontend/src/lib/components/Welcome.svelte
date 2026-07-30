<script lang="ts">
  // Onboarding tour — deliberately self-contained. Nothing else imports this
  // file; it only reaches *outward* (app state, actions, the shortcut labels).
  // Visibility rides on app.modal === "welcome" so the central key dispatcher
  // gives it keyboard ownership for free, and "seen once" lives in localStorage
  // so no backend/settings plumbing leaks the feature into the rest of the app.
  import { onMount } from "svelte";
  import * as act from "../actions";
  import { MOD_LABEL } from "../keys/platform";
  import { app } from "../state/app.svelte";

  const SEEN_KEY = "mynote.welcome.v1";
  const M = MOD_LABEL;

  type Key = { combo: string; label: string };
  type Slide = {
    title: string;
    lead: string;
    gif?: string;
    alt?: string;
    keys: Key[];
    intro?: boolean;
  };

  const slides: Slide[] = [
    {
      title: "Welcome to MyNote",
      lead: "Local-first, keyboard-driven Markdown notes. Your notebook is just a folder of plain .md files — greppable, diffable, and entirely yours.",
      intro: true,
      keys: [],
    },
    {
      title: "Write, insert, preview",
      lead: `${M}+J opens a searchable palette — code blocks, tables, links, task lists, dates. ${M}+E flips between the editor and a live rendered preview.`,
      gif: "/tutorial/edit-preview.gif",
      alt: "Inserting a code block with the palette, then switching to preview",
      keys: [
        { combo: `${M}+J`, label: "Insert palette" },
        { combo: `${M}+E`, label: "Edit / Preview" },
      ],
    },
    {
      title: "Organize without the mouse",
      lead: `${M}+N adds a page, ${M}+Enter a subpage. Fold and unfold subtrees with ← / →, and hop between sections with ${M}+PgUp / PgDn.`,
      gif: "/tutorial/tree-sections.gif",
      alt: "Creating subpages, folding them, switching sections",
      keys: [
        { combo: `${M}+N`, label: "New page" },
        { combo: `${M}+Enter`, label: "New subpage" },
        { combo: `${M}+PgUp / PgDn`, label: "Switch section" },
      ],
    },
    {
      title: "Never memorize a shortcut",
      lead: `Press ? at any time for a context-aware cheat sheet with live search. ${M}+K searches every note — fuzzy or regex — with highlighted snippets.`,
      gif: "/tutorial/help-search.gif",
      alt: "The ? shortcut overlay with live filtering",
      keys: [
        { combo: "?", label: "Shortcut cheat sheet" },
        { combo: `${M}+K`, label: "Search everything" },
      ],
    },
    {
      title: "Optional: full history",
      lead: `With git installed, MyNote snapshots the notebook so every page keeps a timeline of revisions. ${M}+H diffs one against what's on disk and restores it as a single undoable edit; ${M}+Shift+H brings back deleted pages.`,
      gif: "/tutorial/history.gif",
      alt: "Diffing a page against an earlier snapshot, then restoring it",
      keys: [
        { combo: `${M}+H`, label: "Page history" },
        { combo: `${M}+Shift+H`, label: "Deleted pages" },
      ],
    },
    {
      title: "You're all set",
      lead: "A notebook is ready in your default folder — or pick where your notes should live. Every page is plain Markdown you can edit with any tool.",
      keys: [
        { combo: `${M}+O`, label: "Open / switch notebook" },
        { combo: `${M}+I`, label: "Import OneNote .mht" },
        { combo: `${M}+Z / Y`, label: "Undo / redo" },
        { combo: "?", label: "Help anytime" },
      ],
    },
  ];

  let idx = $state(0);
  const slide = $derived(slides[idx]);
  const isLast = $derived(idx === slides.length - 1);

  let wasOpen = false;
  $effect(() => {
    if (app.modal === "welcome") {
      if (!wasOpen) idx = 0; // every fresh open (first run or replay) starts at slide 1
      wasOpen = true;
    } else if (wasOpen) {
      wasOpen = false;
      rememberSeen();
    }
  });

  onMount(() => {
    if (!hasSeen()) app.modal = "welcome";
  });

  function hasSeen(): boolean {
    try {
      return localStorage.getItem(SEEN_KEY) === "1";
    } catch {
      return false;
    }
  }

  function rememberSeen() {
    try {
      localStorage.setItem(SEEN_KEY, "1");
    } catch {
      // private mode / storage disabled — worst case the tour shows again
    }
  }

  function next() {
    if (isLast) act.closeModal();
    else idx += 1;
  }

  function back() {
    if (idx > 0) idx -= 1;
  }

  function chooseNotebook() {
    void act.openNotebookModal();
  }

  function keys(e: KeyboardEvent) {
    if (app.modal !== "welcome") return;
    if (e.key === "ArrowRight" || e.key === "Enter") {
      e.preventDefault();
      next();
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      back();
    }
  }
</script>

<svelte:window onkeydown={keys} />

{#if app.modal === "welcome"}
  <div class="welcome-backdrop">
    <div class="welcome-card" role="dialog" aria-modal="true" aria-label="Welcome to MyNote">
      <button class="welcome-skip" onclick={() => act.closeModal()}>Skip ✕</button>

      <div class="welcome-media" class:intro={slide.intro}>
        {#if slide.gif}
          <img src={slide.gif} alt={slide.alt} />
        {:else}
          <img class="welcome-logo" src="/logo.png" alt="MyNote logo" />
          <div class="welcome-props">No cloud &middot; No network &middot; One small app</div>
        {/if}
      </div>

      <div class="welcome-body">
        <h2>{slide.title}</h2>
        <p>{slide.lead}</p>
        {#if slide.keys.length}
          <div class="welcome-keys">
            {#each slide.keys as k (k.label)}
              <span class="welcome-key"><kbd>{k.combo}</kbd> {k.label}</span>
            {/each}
          </div>
        {/if}
        {#if isLast}
          <button class="welcome-link" onclick={chooseNotebook}>
            Choose where to store your notes…
          </button>
        {/if}
      </div>

      <div class="welcome-foot">
        <div class="welcome-dots">
          {#each slides as _, i (i)}
            <span class="dot" class:on={i === idx}></span>
          {/each}
        </div>
        <div class="welcome-nav">
          <button class="welcome-ghost" disabled={idx === 0} onclick={back}>Back</button>
          <button class="welcome-primary" onclick={next}>{isLast ? "Get started" : "Next"}</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .welcome-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    z-index: 300;
  }
  .welcome-card {
    position: relative;
    width: 640px;
    max-width: 92vw;
    background: var(--panel);
    border: 1px solid var(--guide);
    border-radius: 12px;
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .welcome-skip {
    position: absolute;
    top: 10px;
    right: 10px;
    z-index: 1;
    color: var(--muted);
    font-size: 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
  }
  .welcome-skip:hover {
    color: var(--text);
    background: var(--bg);
  }

  .welcome-media {
    height: 300px;
    background: var(--bg);
    border-bottom: 1px solid var(--guide);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    overflow: hidden;
  }
  .welcome-media img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .welcome-media.intro {
    background: radial-gradient(
      120% 120% at 50% 0%,
      color-mix(in srgb, var(--accent) 12%, var(--bg)),
      var(--bg) 70%
    );
  }
  .welcome-logo {
    width: 104px;
    height: 104px;
  }
  .welcome-props {
    color: var(--muted);
    letter-spacing: 0.04em;
    font-size: 13px;
  }

  .welcome-body {
    padding: 18px 22px 4px;
  }
  .welcome-body h2 {
    margin: 0 0 8px;
    font-size: 20px;
    font-weight: 700;
  }
  .welcome-body p {
    margin: 0;
    color: color-mix(in srgb, var(--text) 82%, transparent);
    line-height: 1.55;
  }
  .welcome-keys {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    margin-top: 16px;
  }
  .welcome-key {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: 13px;
  }
  .welcome-link {
    display: inline-block;
    margin-top: 14px;
    padding: 0;
    color: var(--accent);
    font-size: 13px;
  }
  .welcome-link:hover {
    background: none;
    text-decoration: underline;
  }

  .welcome-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 22px;
    margin-top: 8px;
    border-top: 1px solid var(--guide);
  }
  .welcome-dots {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--text) 22%, transparent);
    transition: width 0.15s ease, background 0.15s ease;
  }
  .dot.on {
    width: 20px;
    background: var(--accent);
  }
  .welcome-nav {
    display: flex;
    gap: 8px;
  }
  .welcome-ghost {
    border: 1px solid var(--guide);
    color: var(--muted);
    padding: 6px 14px;
  }
  .welcome-ghost:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .welcome-primary {
    background: var(--accent);
    color: var(--bg);
    font-weight: 600;
    padding: 6px 16px;
  }
</style>
