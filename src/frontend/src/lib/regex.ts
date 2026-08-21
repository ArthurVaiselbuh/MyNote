const PAGE_LINK = /([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.md$/i;
const ATTACHMENT_LINK = /^files\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\/(.+)$/i;

export function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** The user's find text as a matcher. `null` covers both "nothing to search
 * for" and "an explicit regex that will not compile" — callers that report the
 * second have already ruled out the first. */
export function searchRegex(text: string, isRegex: boolean, caseSensitive = false): RegExp | null {
  if (!text) return null;
  try {
    return new RegExp(isRegex ? text : escapeRegExp(text), caseSensitive ? "g" : "gi");
  } catch {
    return null;
  }
}

export function pageIdFromHref(href: string): string | null {
  return href.match(PAGE_LINK)?.[1] ?? null;
}

export function attachmentFromHref(href: string): { pageId: string; name: string } | null {
  const m = href.match(ATTACHMENT_LINK);
  if (!m) return null;
  try {
    return { pageId: m[1], name: decodeURIComponent(m[2]) };
  } catch {
    return null;
  }
}

export function isExternalHref(href: string): boolean {
  return /^https?:\/\//i.test(href);
}
