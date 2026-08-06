<script lang="ts">
  // Onboarding tour — deliberately self-contained. Nothing else imports this
  // file; it only reaches *outward* (app state, actions, the shortcut labels).
  // Visibility rides on app.modal === "welcome" so the central key dispatcher
  // gives it keyboard ownership for free, and "seen once" lives in localStorage
  // so no backend/settings plumbing leaks the feature into the rest of the app.
  import { onMount } from "svelte";
  import * as act from "../actions";
  import { labelOf } from "../keys/bindings";
  import { clampIndex } from "../listIndex";
  import { app } from "../state/app.svelte";

  const SEEN_KEY = "mynote.welcome.v1";

  type Key = { combo: string; label: string };
  type Slide = {
    title: string;
    lead: string;
    gif?: string;
    alt?: string;
    keys: Key[];
    intro?: boolean;
  };

  // derived, not a plain const: the tour spells the user's own chords so a rebound
  // key never reads as a lie, and settings load after this component mounts
  const slides: Slide[] = $derived([
    {
      title: "Welcome to MyNote",
      lead: "Local-first, keyboard-driven Markdown notes. Your notebook is just a folder of plain .md files — greppable, diffable, and entirely yours.",
      intro: true,
      keys: [],
    },
    {
      title: "Write, insert, preview",
      lead: `${labelOf("app.insertHelper")} opens a searchable palette — code blocks, tables, links, task lists, dates. ${labelOf("app.toggleMode")} flips between the editor and a live rendered preview.`,
      gif: "/tutorial/edit-preview.gif",
      alt: "Inserting a code block with the palette, then switching to preview",
      keys: [
        { combo: labelOf("app.insertHelper"), label: "Insert palette" },
        { combo: labelOf("app.toggleMode"), label: "Edit / Preview" },
      ],
    },
    {
      title: "Organize without the mouse",
      lead: `${labelOf("page.new")} adds a page, ${labelOf("tree.newSubpage")} a subpage. Fold and unfold subtrees with ← / →, and hop between sections with ${labelOf("section.prev")} / ${labelOf("section.next")}.`,
      gif: "/tutorial/tree-sections.gif",
      alt: "Creating subpages, folding them, switching sections",
      keys: [
        { combo: labelOf("page.new"), label: "New page" },
        { combo: labelOf("tree.newSubpage"), label: "New subpage" },
        { combo: `${labelOf("section.prev")} / ${labelOf("section.next")}`, label: "Switch section" },
      ],
    },
    {
      title: "Never memorize a shortcut",
      lead: `Press ${labelOf("app.help")} at any time for a context-aware cheat sheet with live search. ${labelOf("app.search")} searches every note — fuzzy or regex — with highlighted snippets.`,
      gif: "/tutorial/help-search.gif",
      alt: "The ? shortcut overlay with live filtering",
      keys: [
        { combo: labelOf("app.help"), label: "Shortcut cheat sheet" },
        { combo: labelOf("app.search"), label: "Search everything" },
      ],
    },
    {
      title: "Optional: full history",
      lead: `With git installed, MyNote snapshots the notebook so every page keeps a timeline of revisions. ${labelOf("history.open")} diffs one against what's on disk and restores it as a single undoable edit; ${labelOf("history.openDeleted")} brings back deleted pages.`,
      gif: "/tutorial/history.gif",
      alt: "Diffing a page against an earlier snapshot, then restoring it",
      keys: [
        { combo: labelOf("history.open"), label: "Page history" },
        { combo: labelOf("history.openDeleted"), label: "Deleted pages" },
      ],
    },
    {
      title: "You're all set",
      lead: "A notebook is ready in your default folder — or pick where your notes should live. Every page is plain Markdown you can edit with any tool.",
      keys: [
        { combo: labelOf("notebook.open"), label: "Open / switch notebook" },
        { combo: labelOf("notebook.import"), label: "Import OneNote .mht" },
        { combo: `${labelOf("app.undo")} / ${labelOf("app.redo")}`, label: "Undo / redo" },
        { combo: labelOf("app.help"), label: "Help anytime" },
      ],
    },
  ]);

  let idx = $state(0);
  const slide = $derived(slides[idx]);
  const isLast = $derived(idx === slides.length - 1);

  let wasOpen = false;
  $effect(() => {
    const open = app.modal === "welcome";
    if (open === wasOpen) return;
    wasOpen = open;
    if (open) idx = 0;
    else rememberSeen();
  });

  onMount(() => {
    if (!hasSeen()) act.openModal("welcome");
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
    idx = clampIndex(idx - 1, slides.length);
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
          <button class="welcome-link" onclick={() => void act.openNotebookModal()}>
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
