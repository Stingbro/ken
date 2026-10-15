// Assistant-message markdown → sanitized HTML.
import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({ gfm: true, breaks: true });

export function renderMarkdown(md: string): string {
  const html = marked.parse(md, { async: false }) as string;
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: [
      "p", "br", "strong", "em", "del", "code", "pre", "blockquote",
      "ul", "ol", "li", "h1", "h2", "h3", "h4", "a", "table", "thead",
      "tbody", "tr", "th", "td", "hr",
    ],
    ALLOWED_ATTR: ["href"],
    // DOMPurify default URIs, plus ken:// so a cited source stays a link.
    ALLOWED_URI_REGEXP: /^(?:(?:(?:f|ht)tps?|mailto|tel|ken):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
  });
}

// Search snippets carry only the backend's <mark> highlight tags. Escape the
// whole string, then re-allow just <mark>/</mark> — the result is fed to Svelte
// {@html}, so every other tag (script, img onerror, …) must stay inert. This is
// the single XSS boundary used by SearchOverlay; fix it once.
export function renderSearchSnippet(snippet: string): string {
  return snippet
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("&lt;mark&gt;", "<mark>")
    .replaceAll("&lt;/mark&gt;", "</mark>");
}

const escapeHtml = (s: string) => s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

/** A one-line title (a ticket, a ruling, an Inbox item, a Log entry) with its
 *  `code` spans shown as code and everything else as escaped text. `mark`
 *  renders each plain run and must escape it; the Log passes its search
 *  highlight. Fed to Svelte {@html}, so nothing else is let through. */
export function renderTitle(text: string, mark: (s: string) => string = escapeHtml): string {
  return text
    .split(/`([^`\n]+)`/)
    .map((part, i) => (i % 2 === 1 ? `<code class="title-code">${mark(part)}</code>` : mark(part)))
    .join("");
}

/** The href a `[[page]]` link gets in `renderBody`, before the page's name. */
const PAGE_HREF = "#ken-page:";

/** A body (a finding's, a card's) as sanitized HTML: Markdown, with each
 *  `[[page]]` or `[[page|label]]` a link. The caller follows a click on
 *  one through `pageLinkTarget`. */
export function renderBody(md: string): string {
  const linked = md.replace(/\[\[([^\]\n|]+)(?:\|([^\]\n]+))?\]\]/g, (_, target: string, label: string | undefined) => {
    const text = (label ?? target).trim().replace(/[[\]]/g, "");
    return `[${text}](${PAGE_HREF}${encodeURIComponent(target.trim())})`;
  });
  return renderMarkdown(linked);
}

/** The page a link `renderBody` made names; null for any other link. */
export function pageLinkTarget(href: string | null): string | null {
  if (!href?.startsWith(PAGE_HREF)) return null;
  return decodeURIComponent(href.slice(PAGE_HREF.length)) || null;
}
