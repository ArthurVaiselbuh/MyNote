<script lang="ts">
  import * as act from "../../actions";
  import { autofocusSelect } from "../../autofocus";
  import { labelOf } from "../../keys/bindings";
  import { clampIndex } from "../../listIndex";
  import { COLOR_PALETTE, COLOR_PALETTE_GRADIENT } from "../../markdown";
  import { editorCtl } from "../../paneCtl";

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

  const openedAt = new Date();
  const date = `${openedAt.getFullYear()}-${pad(openedAt.getMonth() + 1)}-${pad(openedAt.getDate())}`;
  const dateTime = `${date} ${pad(openedAt.getHours())}:${pad(openedAt.getMinutes())}`;

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
    { label: "Date", before: date, after: "", hint: date },
    { label: "Date + time", before: dateTime, after: "", hint: dateTime },
    {
      label: "Image width",
      before: "",
      after: "{width=420}",
      hint: "![alt](src){width=420}",
      search: "size resize img picture",
    },
    {
      label: "Text size",
      before: "[",
      after: "]{size=12}",
      hint: "[text]{size=12}",
      search: "font smaller larger preview",
    },
    {
      label: "Code block size",
      before: "``` {size=12}\n",
      after: "\n```",
      hint: "```ts {size=12}",
      search: "font smaller larger preview fenced",
    },
    {
      label: "Color…",
      before: "",
      after: "",
      hint: "[text]{.red}",
      swatch: COLOR_PALETTE_GRADIENT,
      search: `colour text ${Object.keys(COLOR_PALETTE).join(" ")} grey`,
      action: () => act.openModal("colorPicker"),
    },
    {
      label: "Attach file…",
      before: "",
      after: "",
      hint: "[name](files/…)",
      search: "attachment attach upload pdf document file",
      action: () => {
        act.closeModal();
        act.attachFile();
      },
    },
  ];

  const chord = $derived(labelOf("app.insertHelper"));

  let filter = $state("");
  let sel = $state(0);
  let listEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    (listEl?.children[sel] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
  });

  const items = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return ITEMS.filter(
      (item) =>
        !f || item.label.toLowerCase().includes(f) || (item.search?.includes(f) ?? false),
    );
  });

  $effect(() => {
    if (sel >= items.length) sel = clampIndex(sel, items.length);
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
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      sel = clampIndex(sel + (e.key === "ArrowDown" ? 1 : -1), items.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (items[sel]) choose(items[sel]);
    }
  }
</script>

<div class="modal-backdrop">
  <div class="modal" style:width="440px" role="dialog">
    <div class="modal-title">Insert{chord ? ` (${chord})` : ""}</div>
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
