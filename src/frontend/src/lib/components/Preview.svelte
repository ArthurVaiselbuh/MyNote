<script lang="ts">
  import * as act from "../actions";
  import { renderBody } from "../markdown";

  let { body }: { body: string } = $props();

  const html = $derived(renderBody(body));

  function onClick(e: MouseEvent) {
    const link = (e.target as HTMLElement).closest("a");
    if (!link) return;
    e.preventDefault();
    const href = link.getAttribute("href") ?? "";
    const match = href.match(/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.md$/i);
    if (match) act.openPageById(match[1]);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="preview" id="preview-scroll" onclick={onClick}>
  {@html html}
</div>
