/**
 * Content-proportional column widths for Markdown tables.
 *
 * A table in Ken never scrolls sideways: it is capped at the text column (or,
 * in full-width mode, at the pane's gutters) and its cells wrap. Getting that
 * guarantee out of CSS alone needs `overflow-wrap: anywhere` on the cells,
 * because only `anywhere` lets a single unbreakable token (a URL, an id) give
 * way instead of forcing the table wider than its cap.
 *
 * The price is that `anywhere` also collapses every column's *min-content*
 * width to about one character, and the browser's automatic table layout
 * sizes columns by interpolating between min-content and max-content. With
 * every floor at one character it ends up splitting the width almost evenly:
 * a `2026-09-17` column wrapped onto three lines while a prose column that
 * wanted 1100px was handed 100px. Proportion was exactly what got lost.
 *
 * So the widths are computed here instead, from each column's max-content
 * width — the width its longest cell (or its heading) would want on one line,
 * i.e. how much content the column holds — and written to a `<colgroup>` with
 * `table-layout: fixed`, which makes them exact. See `columnWidths` for the
 * rule.
 *
 * Widths are DOM-only: the `<colgroup>` sits outside ProseMirror's content
 * DOM and is never serialized, so the markdown round trip is untouched. They
 * are recomputed on every document change and whenever the editor is resized.
 */
import { Plugin, PluginKey } from "@milkdown/kit/prose/state";
import { $prose } from "@milkdown/kit/utils";

/** Our colgroup, so it is never confused with anything Crepe adds. */
const COLGROUP_CLASS = "ken-table-cols";

/**
 * No column is narrower than this, whatever the arithmetic says: below it a
 * cell is all padding and the text has nowhere to go.
 */
const MIN_COLUMN = 28;

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

/** `xs`, scaled so it adds up to `total`. */
function scaleTo(xs: number[], total: number): number[] {
  const s = sum(xs);
  if (s <= 0) return xs.map(() => total / Math.max(1, xs.length));
  return xs.map((x) => (x * total) / s);
}

/**
 * Hand `leftover` to the columns still below their max-content width, in
 * proportion to that max-content — repeatedly, because a column that reaches
 * its max-content drops out and its share passes to the others.
 */
function distributeSurplus(
  widths: number[],
  max: number[],
  leftover: number,
): void {
  let remaining = leftover;
  for (let pass = 0; pass < 4 && remaining > 0.5; pass++) {
    const open: number[] = [];
    for (let i = 0; i < widths.length; i++) {
      if (widths[i]! < max[i]! - 0.5) open.push(i);
    }
    if (open.length === 0) return;
    const weight = sum(open.map((i) => max[i]!));
    if (weight <= 0) return;
    let used = 0;
    for (const i of open) {
      const give = Math.min(
        (remaining * max[i]!) / weight,
        max[i]! - widths[i]!,
      );
      widths[i] = widths[i]! + give;
      used += give;
    }
    remaining -= used;
  }
}

/**
 * Lift any column that came out under `floor` back up to it, and take the
 * difference back from the columns that have room above the floor, in
 * proportion to how much room that is. Keeps the total unchanged.
 */
function enforceFloor(widths: number[], floor: number): void {
  let deficit = 0;
  for (let i = 0; i < widths.length; i++) {
    if (widths[i]! < floor) {
      deficit += floor - widths[i]!;
      widths[i] = floor;
    }
  }
  for (let pass = 0; pass < 4 && deficit > 0.5; pass++) {
    const open: number[] = [];
    for (let i = 0; i < widths.length; i++) {
      if (widths[i]! > floor + 0.5) open.push(i);
    }
    const weight = sum(open.map((i) => widths[i]! - floor));
    if (weight <= 0) return;
    let taken = 0;
    for (const i of open) {
      const take = Math.min(
        (deficit * (widths[i]! - floor)) / weight,
        widths[i]! - floor,
      );
      widths[i] = widths[i]! - take;
      taken += take;
    }
    deficit -= taken;
  }
}

/**
 * How a squeezed column's share is weighted. Sharing the space in proportion
 * to content alone is too harsh on the small columns — in a table whose prose
 * column holds twenty times the text of its date column, a linear split gives
 * the date column a twentieth of the room and it wraps into a stack. The
 * square root keeps the *ordering* by content (a column with more text is
 * always wider) while compressing the ratios, so the wide columns give up
 * proportionally more, which is what a reader wants: long prose re-wraps
 * gracefully, `2026-09-17` does not.
 */
const weigh = Math.sqrt;

/**
 * Column widths for a table whose columns want `max` (their max-content
 * widths, headings included), fitted into `cap`. `stretch` is true for
 * full-width tables, which fill their cap even when their content would fit
 * in less; a default-width table shrinks to its content.
 *
 * Exported for unit tests.
 *
 * The rule, when the content does not all fit: protect as many of the
 * narrowest columns as the cap can afford, and let the rest absorb the
 * compression.
 *
 *   - Sort the columns by content and consider protecting the `k` narrowest,
 *     for every `k`: those get their *full* max-content width, and the
 *     remainder is split between the others by `weigh`.
 *   - A `k` is allowed only if every squeezed column still comes out at least
 *     as wide as the widest protected one. So a column is protected exactly
 *     when its whole content fits in the room a squeezed column gets anyway:
 *     no column is ever held wide at the expense of one with more in it.
 *   - Take the largest allowed `k`. Dates, counts, statuses and names are
 *     then rendered whole, and the description and notes columns — which
 *     re-wrap gracefully — give up the difference between them, by content.
 *
 * `k = 0` always qualifies, so there is always an answer; when the cap is
 * tight enough that nothing can be protected (fourteen columns in a 630px
 * pane, say) every column simply gets its weighted share. Either way no
 * column ends up under `MIN_COLUMN`, and mid-word breaking comes from the
 * cells' `overflow-wrap: anywhere`, so the table still does not scroll.
 */
export function columnWidths(
  max: number[],
  cap: number,
  stretch: boolean,
): number[] {
  const n = max.length;
  if (n === 0) return [];
  const floor = Math.min(MIN_COLUMN, cap / n);
  const mc = max.map((x) => Math.max(x, floor));

  if (sum(mc) <= cap) return stretch ? scaleTo(mc, cap) : mc;

  const byContent = mc.map((_, i) => i).sort((a, b) => mc[a]! - mc[b]!);

  /**
   * Widths with the `k` narrowest columns protected, or `undefined` if that
   * many cannot be: either there is nothing left to share, or a squeezed
   * column would come out narrower than a protected one, which would put a
   * column with less content in it in the wider seat.
   */
  const withProtected = (k: number): number[] | undefined => {
    const protectedCols = byContent.slice(0, k);
    const squeezed = byContent.slice(k);
    const leftover = cap - sum(protectedCols.map((i) => mc[i]!));
    if (squeezed.length === 0 || leftover <= 0) return undefined;
    const widest = k === 0 ? 0 : mc[protectedCols[k - 1]!]!;
    const weight = sum(squeezed.map((i) => weigh(mc[i]!)));
    const widths = mc.slice();
    for (const i of squeezed) {
      const share = (leftover * weigh(mc[i]!)) / weight;
      if (share < widest) return undefined;
      widths[i] = Math.min(share, mc[i]!);
    }
    distributeSurplus(widths, mc, cap - sum(widths));
    enforceFloor(widths, floor);
    return widths;
  };

  // Few enough columns that trying every `k` outright beats reasoning about
  // whether the test is monotone in `k` — it is not quite, because a column
  // moving into the protected set lifts every remaining share.
  let best = scaleTo(mc, cap);
  enforceFloor(best, floor);
  for (let k = 0; k < n; k++) {
    const widths = withProtected(k);
    if (widths) best = widths;
  }
  return best;
}

// ---------------------------------------------------------------------------
// Measuring and applying, against the live DOM.
// ---------------------------------------------------------------------------

interface TableParts {
  block: HTMLElement;
  wrapper: HTMLElement;
  table: HTMLTableElement;
  headerRow: HTMLTableRowElement;
}

function partsOf(block: Element): TableParts | undefined {
  if (!(block instanceof HTMLElement)) return undefined;
  const table = block.querySelector<HTMLTableElement>(":scope table.children");
  const wrapper = table?.parentElement;
  const headerRow = table?.tBodies[0]?.rows[0] ?? table?.rows[0];
  if (!table || !(wrapper instanceof HTMLElement) || !headerRow) {
    return undefined;
  }
  return { block, wrapper, table, headerRow };
}

const horizontalPadding = (el: HTMLElement) => {
  const cs = getComputedStyle(el);
  return parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
};

/**
 * Each column's max-content width, plus the width the columns have to fit
 * into. The table is laid out at `max-content` to read the former;
 * everything is restored before returning, and because the whole function
 * runs inside one task the intermediate layout is never painted.
 */
function measure(
  parts: TableParts,
): { max: number[]; cap: number } | undefined {
  const { block, wrapper, table, headerRow } = parts;
  const full = block.getAttribute("data-full-width") === "true";

  table.querySelector(`:scope > colgroup.${COLGROUP_CLASS}`)?.remove();

  // A default-width block is `width: fit-content`, so it is only as wide as
  // the table it holds and cannot report the space available: widen it to its
  // containing block first. A full-width block already has an explicit width.
  const blockWidth = block.style.width;
  if (!full) block.style.width = "100%";
  const tableStyle = table.getAttribute("style") ?? "";

  const available = wrapper.clientWidth - horizontalPadding(wrapper);

  table.style.tableLayout = "auto";
  table.style.minWidth = "0";
  table.style.maxWidth = "none";
  table.style.width = "max-content";
  const max = Array.from(
    headerRow.cells,
    (c) => c.getBoundingClientRect().width,
  );
  // A table is a shade wider than its columns — its own border, and the
  // border spacing between them under `border-collapse: separate`. Charging
  // that to the budget is what keeps the table from ending up a pixel over
  // its cap, which would count as an overflow.
  const chrome = Math.max(0, table.getBoundingClientRect().width - sum(max));

  if (tableStyle) table.setAttribute("style", tableStyle);
  else table.removeAttribute("style");
  block.style.width = blockWidth;

  const cap = available - chrome;
  if (!(cap > 0) || max.length === 0) return undefined;
  return { max, cap };
}

/** Write the computed widths, unless they are already what the table has. */
function apply(parts: TableParts, widths: number[], total: number): void {
  const { table } = parts;
  const want = widths.map((w) => `${w.toFixed(2)}px`);
  let colgroup = table.querySelector<HTMLElement>(
    `:scope > colgroup.${COLGROUP_CLASS}`,
  );
  const have = colgroup
    ? Array.from(colgroup.children, (c) => (c as HTMLElement).style.width)
    : [];
  if (have.length !== want.length || have.some((h, i) => h !== want[i])) {
    if (!colgroup) {
      colgroup = document.createElement("colgroup");
      colgroup.className = COLGROUP_CLASS;
      // Never editable, and never something ProseMirror should read back.
      colgroup.contentEditable = "false";
      table.insertBefore(colgroup, table.firstChild);
    }
    colgroup.replaceChildren(
      ...want.map((w) => {
        const col = document.createElement("col");
        col.style.width = w;
        return col;
      }),
    );
  }
  const totalPx = `${total.toFixed(2)}px`;
  if (table.style.width !== totalPx) table.style.width = totalPx;
  if (table.style.tableLayout !== "fixed") table.style.tableLayout = "fixed";
  if (table.style.maxWidth !== "none") table.style.maxWidth = "none";
}

/** Measure and size every table under `root`. */
export function layoutTables(root: ParentNode): void {
  for (const block of root.querySelectorAll(".milkdown-table-block")) {
    const parts = partsOf(block);
    if (!parts) continue;
    const m = measure(parts);
    if (!m) continue;
    const stretch = parts.block.getAttribute("data-full-width") === "true";
    const widths = columnWidths(m.max, m.cap, stretch);
    apply(parts, widths, Math.min(sum(widths), m.cap));
  }
}

/**
 * Keep every table's columns sized to its content. One pass per animation
 * frame at most: document changes, node-view mounts and pane resizes all
 * funnel into the same scheduled run.
 */
export const tableLayoutPlugin = $prose(
  () =>
    new Plugin({
      key: new PluginKey("ken-table-layout"),
      view: (view) => {
        let frame = 0;
        // Our own writes change a block's width, which the ResizeObserver
        // would see as a resize: ignore observations we caused ourselves.
        let running = false;
        const run = () => {
          frame = 0;
          running = true;
          try {
            layoutTables(view.dom);
          } finally {
            // A microtask is too early: the observer fires after layout.
            requestAnimationFrame(() => {
              running = false;
            });
          }
        };
        const schedule = () => {
          if (frame === 0) frame = requestAnimationFrame(run);
        };
        const observer = new ResizeObserver(() => {
          if (!running) schedule();
        });
        observer.observe(view.dom);
        // Web fonts land after the first layout and change every measurement.
        document.fonts?.ready.then(schedule).catch(() => {});
        schedule();
        return {
          update: schedule,
          destroy: () => {
            observer.disconnect();
            if (frame !== 0) cancelAnimationFrame(frame);
          },
        };
      },
    }),
);
