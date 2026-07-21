<script lang="ts">
  import * as act from "../../actions";
  import { autofocusSelect } from "../../autofocus";
  import { editorCtl } from "../../editorCtl";
  import { MOD_LABEL } from "../../keys/platform";
  import { COLOR_PALETTE } from "../../markdown";

  interface Item {
    label: string;
    before: string;
    after: string;
    hint: string;
    swatch?: string;
    search?: string;
    action?: () => void;
  }

  function pad(n: number): string {
    return String(n).padStart(2, "0");
  }
  function today(): string {
    const d = new Date();
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  function nowStamp(): string {
    const d = new Date();
    return `${today()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  const ITEMS: Item[] = [
    { label: "Link", before: "[", after: "](url)", hint: "[text](url)" },
    { label: "Bold", before: "**", after: "**", hint: "**text**" },
    { label: "Italic", before: "_", after: "_", hint: "_text_" },
    { label: "Strikethrough", before: "~~", after: "~~", hint: "~~text~~" },
    { label: "Inline code", before: "`", after: "`", hint: "`code`" },
    { label: "Heading 1", before: "# ", after: "", hint: "# " },
    { label: "Heading 2", before: "## ", after: "", hint: "## " },
    { label: "Heading 3", before: "### ", after: "", hint: "### " },
    { label: "Bullet list", before: "- ", after: "", hint: "- item" },
    { label: "Numbered list", before: "1. ", after: "", hint: "1. item" },
    { label: "Task", before: "- [ ] ", after: "", hint: "- [ ] todo" },
    { label: "Quote", before: "> ", after: "", hint: "> quote" },
    { label: "Code block", before: "```\n", after: "\n```", hint: "``` … ```" },
    { label: "Table", before: "| A | B |\n| --- | --- |\n|  |  |\n", after: "", hint: "2×2 table" },
    { label: "Divider", before: "\n---\n", after: "", hint: "---" },
    { label: "Date", before: today(), after: "", hint: today() },
    { label: "Date + time", before: nowStamp(), after: "", hint: nowStamp() },
    {
      label: "Image width",
      before: "",
      after: "{width=420}",
      hint: "![alt](src){width=420}",
      search: "size resize img picture",
    },
    {
      label: "Color…",
      before: "",
      after: "",
      hint: "[text]{.red}",
      swatch: `linear-gradient(90deg, ${Object.values(COLOR_PALETTE).join(",")})`,
      search: `colour text ${Object.keys(COLOR_PALETTE).join(" ")} grey`,
      action: () => act.openModal("colorPicker"),
    },
  ];

  let filter = $state("");
  let sel = $state(0);
  let listEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    void sel;
    listEl?.querySelector(".insert-item.selected")?.scrollIntoView({ block: "nearest" });
  });

  const items = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return ITEMS.filter(
      (item) =>
        !f || item.label.toLowerCase().includes(f) || (item.search?.includes(f) ?? false),
    );
  });

  $effect(() => {
    if (sel >= items.length) sel = Math.max(0, items.length - 1);
  });

  function choose(item: Item) {
    if (item.action) {
      item.action();
      return;
    }
    act.closeModal();
    editorCtl.current?.insert(item.before, item.after);
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      sel = Math.min(items.length - 1, sel + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (items[sel]) choose(items[sel]);
    }
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="440px" role="dialog">
    <div class="modal-title">Insert ({MOD_LABEL}+J)</div>
    <input
      placeholder="filter…"
      style="width:100%"
      bind:value={filter}
      use:autofocusSelect
      onkeydown={keys}
    />
    <div class="insert-list" bind:this={listEl}>
      {#each items as item, i (item.label)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="insert-item"
          class:selected={i === sel}
          onclick={() => choose(item)}
          onmousemove={() => (sel = i)}
        >
          <span>
            {#if item.swatch}<span class="swatch" style:background={item.swatch}></span>{/if}
            {item.label}
          </span>
          <span class="preview-snippet" style:color={item.swatch}>{item.hint}</span>
        </div>
      {/each}
    </div>
    <div class="hint">↑↓ select · Enter insert (wraps selection) · Esc close</div>
  </div>
</div>
