// Backspace at the very start of a heading should turn it into a paragraph in
// one press. Milkdown's default `DowngradeHeading` shortcut steps one level at
// a time (h3 → h2 → h1 → p), so we disable that shortcut and install our own.
import type { Ctx } from "@milkdown/kit/ctx";
import { setBlockType } from "@milkdown/kit/prose/commands";
import { keymap } from "@milkdown/kit/prose/keymap";
import {
  headingKeymap,
  headingSchema,
  paragraphSchema,
} from "@milkdown/kit/preset/commonmark";
import * as milkdown from "@milkdown/kit/utils";

/**
 * Drop Milkdown's stepwise heading downgrade. Pass this to
 * `editor.config(...)`; it must run before the editor state is built.
 */
export function disableHeadingDowngrade(ctx: Ctx) {
  ctx.update(headingKeymap.key, (keys) => ({
    ...keys,
    DowngradeHeading: { ...keys.DowngradeHeading, shortcuts: [] },
  }));
}

/**
 * Backspace on an empty selection at offset 0 inside a heading of any level
 * converts the whole block to a paragraph. `$prose` plugins are prepended to
 * the plugin list that `editorState` builds (`[...prosePlugins, …,
 * createKeymap(km.build())]`), so this keymap is consulted before ProseMirror's
 * base keymap — which would otherwise join the heading into the block above.
 */
export const headingBackspace = milkdown.$prose((ctx: Ctx) =>
  keymap({
    Backspace: (state, dispatch) => {
      const { selection } = state;
      if (!selection.empty) return false;
      const { $from } = selection;
      if ($from.parentOffset !== 0) return false;
      if ($from.parent.type !== headingSchema.type(ctx)) return false;
      return setBlockType(paragraphSchema.type(ctx))(state, dispatch);
    },
  }),
);
