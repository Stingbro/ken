// Typing `/h1 `, `/quote `, `/note ` … at the start of an otherwise empty
// paragraph or heading applies that block type straight away, without opening
// the slash menu. The parsing half is pure so it can be unit tested; the
// ProseMirror half is a Milkdown input rule.
import {
  blockquoteSchema,
  bulletListSchema,
  codeBlockSchema,
  headingSchema,
  hrSchema,
  listItemSchema,
  orderedListSchema,
  paragraphSchema,
} from "@milkdown/kit/preset/commonmark";
import { InputRule } from "@milkdown/kit/prose/inputrules";
import type { NodeType } from "@milkdown/kit/prose/model";
import {
  Selection,
  TextSelection,
  type Transaction,
} from "@milkdown/kit/prose/state";
import { findWrapping } from "@milkdown/kit/prose/transform";
import * as milkdown from "@milkdown/kit/utils";
import { collectHeadings } from "./anchors";
import { buildTocList } from "./toc";

export type AlertKind = "note" | "tip" | "important" | "warning" | "caution";

export type SlashShortcut =
  | { type: "heading"; level: number }
  | { type: "quote" }
  | { type: "code" }
  | { type: "divider" }
  | { type: "bulletList" }
  | { type: "orderedList" }
  | { type: "taskList" }
  | { type: "toc" }
  | { type: "alert"; kind: AlertKind };

/** The whole block text must be exactly the shortcut plus its trailing space. */
export const SLASH_SHORTCUT_RE =
  /^\/(h[1-6]|quote|code|divider|bullet|number|todo|toc|note|tip|important|warning|caution)\s$/i;

const ALERT_KINDS: readonly string[] = [
  "note",
  "tip",
  "important",
  "warning",
  "caution",
];

/**
 * Parse a block's full text (including the space that triggered the rule) into
 * the block type it asks for, or null when it is not a slash shortcut.
 */
export function parseSlashShortcut(text: string): SlashShortcut | null {
  const match = SLASH_SHORTCUT_RE.exec(text);
  if (!match) return null;
  const keyword = match[1].toLowerCase();

  if (keyword.startsWith("h")) {
    return { type: "heading", level: Number(keyword.slice(1)) };
  }
  if (ALERT_KINDS.includes(keyword)) {
    return { type: "alert", kind: keyword as AlertKind };
  }
  switch (keyword) {
    case "quote":
      return { type: "quote" };
    case "code":
      return { type: "code" };
    case "divider":
      return { type: "divider" };
    case "bullet":
      return { type: "bulletList" };
    case "number":
      return { type: "orderedList" };
    case "todo":
      return { type: "taskList" };
    case "toc":
      return { type: "toc" };
    default:
      return null;
  }
}

/** Wrap the (already emptied) textblock at `pos` in `type`, list items included. */
function wrapBlock(
  tr: Transaction,
  pos: number,
  type: NodeType,
  attrs: Record<string, unknown> | null = null,
): boolean {
  const $pos = tr.doc.resolve(pos);
  const range = $pos.blockRange();
  if (!range) return false;
  const wrapping = findWrapping(range, type, attrs);
  if (!wrapping) return false;
  tr.wrap(range, wrapping);
  return true;
}

/**
 * Input rule for the slash shortcuts. Alerts are looked up by name at run time
 * so this module stays independent of the GitHub-alert plugin: without it the
 * rule simply does not fire and the typed text is left alone.
 */
export const slashShortcutInputRule = milkdown.$inputRule(
  (ctx) =>
    new InputRule(SLASH_SHORTCUT_RE, (state, match, start, end) => {
      const shortcut = parseSlashShortcut(match[0]);
      if (!shortcut) return null;

      const $start = state.doc.resolve(start);
      const parent = $start.parent;
      if (parent.type.name !== "paragraph" && parent.type.name !== "heading") {
        return null;
      }
      // The shortcut has to be the entire block, not a fragment of a sentence.
      if (start !== $start.start() || end !== $start.end()) return null;

      const paragraph = paragraphSchema.type(ctx);
      const tr = state.tr.delete(start, end);
      // After the delete the block is empty; `pos` is inside it.
      const pos = tr.mapping.map(start);
      // Headings can be wrapped/turned too, so normalise to a paragraph first.
      if (parent.type !== paragraph) {
        tr.setBlockType(pos, pos, paragraph);
      }

      switch (shortcut.type) {
        case "heading": {
          tr.setBlockType(pos, pos, headingSchema.type(ctx), {
            level: shortcut.level,
          });
          break;
        }
        case "quote": {
          if (!wrapBlock(tr, pos, blockquoteSchema.type(ctx))) return null;
          break;
        }
        case "code": {
          tr.setBlockType(pos, pos, codeBlockSchema.type(ctx));
          break;
        }
        case "divider": {
          const $pos = tr.doc.resolve(pos);
          tr.replaceWith($pos.before(), $pos.after(), [
            hrSchema.type(ctx).create(),
            paragraph.create(),
          ]);
          const sel = TextSelection.near(tr.doc.resolve(pos + 1));
          tr.setSelection(sel);
          break;
        }
        case "bulletList": {
          if (!wrapBlock(tr, pos, bulletListSchema.type(ctx))) return null;
          break;
        }
        case "orderedList": {
          if (!wrapBlock(tr, pos, orderedListSchema.type(ctx))) return null;
          break;
        }
        case "taskList": {
          if (!wrapBlock(tr, pos, listItemSchema.type(ctx), { checked: false }))
            return null;
          break;
        }
        case "toc": {
          // Without the TOC plugin registered the `bullet_list` node has no
          // `toc` attribute, and the shortcut would produce a plain list: do
          // nothing instead and leave the typed text alone.
          const bulletList = bulletListSchema.type(ctx);
          if (!bulletList.spec.attrs?.toc) return null;
          const $pos = tr.doc.resolve(pos);
          const from = $pos.before();
          const list = buildTocList(state.schema, collectHeadings(tr.doc));
          tr.replaceWith(from, $pos.after(), list);
          const after = Math.min(
            from + list.nodeSize,
            tr.doc.content.size,
          );
          tr.setSelection(Selection.near(tr.doc.resolve(after), 1));
          break;
        }
        case "alert": {
          const alert = state.schema.nodes[`github_alert`];
          if (!alert) return null;
          const $pos = tr.doc.resolve(pos);
          const node = alert.createAndFill({ kind: shortcut.kind });
          if (!node) return null;
          tr.replaceWith($pos.before(), $pos.after(), node);
          tr.setSelection(TextSelection.near(tr.doc.resolve(pos + 1)));
          break;
        }
      }

      return tr;
    }),
);
