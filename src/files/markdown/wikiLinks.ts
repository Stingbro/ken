/**
 * `[[Wiki links]]` in the Markdown editor: shown as links, and a click opens
 * the page. The editor keeps them as text (Milkdown has no wiki-link node),
 * so a decoration marks each one and a click on it is resolved to a file.
 */
import { Plugin, PluginKey } from "@milkdown/kit/prose/state";
import { Decoration, DecorationSet } from "@milkdown/kit/prose/view";
import { $prose } from "@milkdown/kit/utils";

const LINK = /\[\[([^\]\n]+?)\]\]/g;

/** The page a link names: `[[Name]]`, `[[Name|label]]`, `[[Name#Heading]]`
 *  or `[[Folder/Name]]`. A bare name is the page of that file name, in the
 *  linking page's folder first, then the shortest path; a path is matched
 *  from the repo's root, then from the linking page's folder. */
export function resolveWikiLink(target: string, fromPath: string, files: string[]): string | null {
  const name = target.split("|")[0].split("#")[0].trim().replace(/\\/g, "/");
  if (!name) return null;
  const pages = files.filter((f) => /\.(md|markdown)$/i.test(f));
  const lower = (s: string) => s.toLowerCase();
  const noExt = (s: string) => s.replace(/\.(md|markdown)$/i, "");
  const dir = fromPath.includes("/") ? fromPath.slice(0, fromPath.lastIndexOf("/")) : "";
  if (name.includes("/")) {
    const want = lower(noExt(name)).replace(/^\.?\//, "");
    const fromDir = lower(dir ? `${dir}/${want}` : want);
    return (
      pages.find((p) => lower(noExt(p)) === want) ??
      pages.find((p) => lower(noExt(p)) === fromDir) ??
      pages.find((p) => lower(noExt(p)).endsWith(`/${want}`)) ??
      null
    );
  }
  const want = lower(noExt(name));
  const same = pages.filter((p) => lower(noExt(p.split("/").pop() ?? p)) === want);
  if (same.length === 0) return null;
  return (
    same.find((p) => lower(p.slice(0, Math.max(0, p.lastIndexOf("/")))) === lower(dir)) ??
    [...same].sort((a, b) => a.split("/").length - b.split("/").length || a.length - b.length)[0]
  );
}

/** The link at offset `at` of `text`, if one covers it. */
export function linkAt(text: string, at: number): string | null {
  for (const m of text.matchAll(LINK)) {
    const start = m.index ?? 0;
    if (at >= start && at <= start + m[0].length) return m[1];
  }
  return null;
}

const key = new PluginKey("ken-wiki-links");

/** The plugin: `open` gets the link's target when one is clicked. */
export function wikiLinkPlugin(open: (target: string) => void) {
  return $prose(
    () =>
      new Plugin({
        key,
        props: {
          decorations(state) {
            const decos: Decoration[] = [];
            state.doc.descendants((node, pos) => {
              if (!node.isText || !node.text) return;
              for (const m of node.text.matchAll(LINK)) {
                const from = pos + (m.index ?? 0);
                decos.push(Decoration.inline(from, from + m[0].length, { class: "ken-wikilink", "data-target": m[1] }));
              }
            });
            return DecorationSet.create(state.doc, decos);
          },
          handleClick(view, pos, event) {
            const el = event.target as HTMLElement | null;
            const marked = el?.closest?.(".ken-wikilink") as HTMLElement | null;
            let target = marked?.dataset.target ?? null;
            if (!target) {
              const $pos = view.state.doc.resolve(pos);
              const node = $pos.parent.child($pos.index());
              if (node?.isText && node.text) target = linkAt(node.text, $pos.textOffset);
            }
            if (!target) return false;
            event.preventDefault();
            open(target);
            return true;
          },
        },
      }),
  );
}
