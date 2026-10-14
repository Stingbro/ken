// The workspace settings screen, the pure part (knowledge-core decision 37):
// how a finding of the knowledge base's checks is labelled, the sources no one
// has said the kind of and the kinds set-up would give them, and how the
// editors read and show the tiers, roles and controls of `.ken/knowledge.json`.
import type { ControlRow, Finding, RepoKind, SetupRepoRow, SourceRow } from "./api";

/** The tag a finding row carries. */
export function findingLabel(kind: string): string {
  switch (kind) {
    case "drift":
      return "drift";
    case "aged":
      return "not checked";
    case "supersession":
      return "supersession";
    case "unknown-ruling":
      return "ruling";
    case "contradiction":
      return "contradiction";
    case "broken-link":
    case "missing-page":
    case "ambiguous-name":
    case "orphan":
      return "links";
    case "suggested-page":
      return "page to write";
    default:
      return kind || "finding";
  }
}

/** Findings in the order the settings screen lists them: drift first, then pages
 *  not checked in a month, superseded rulings, contradictions, rulings cited in
 *  code, links, pages to write and anything else. Stable within a kind. */
export function orderFindings(findings: readonly Finding[]): Finding[] {
  const order = ["drift", "not checked", "supersession", "contradiction", "ruling", "links", "page to write"];
  const rank = (k: string) => order.indexOf(findingLabel(k));
  return findings
    .map((f, i) => ({ f, i, r: rank(f.kind) }))
    .sort((a, b) => (a.r === -1 ? 9 : a.r) - (b.r === -1 ? 9 : b.r) || a.i - b.i)
    .map(({ f }) => f);
}

/** Repos no one has said the kind of. */
export function kindlessRepos<T extends Pick<SourceRow, "kind" | "available">>(repos: readonly T[]): T[] {
  return repos.filter((r) => r.available && r.kind.length === 0);
}

function samePath(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
  return norm(a) === norm(b);
}

/** The kind set-up proposes for each repo, by repo id: the proposal row with
 *  the same folder, else the same member name. A repo the proposal does not
 *  cover, or covers with no kind, is left out. */
export function proposedKinds(
  repos: readonly Pick<SourceRow, "id" | "name" | "path">[],
  rows: readonly Pick<SetupRepoRow, "member" | "path" | "kind">[],
): Record<string, RepoKind[]> {
  const out: Record<string, RepoKind[]> = {};
  for (const r of repos) {
    const row =
      rows.find((x) => samePath(x.path, r.path)) ??
      rows.find((x) => x.member === r.name || x.member.split("/").pop() === r.name.split("/").pop());
    if (row && row.kind.length > 0) out[r.id] = [...row.kind];
  }
  return out;
}

/** A text area's lines, trimmed, blank ones dropped: what the editors send. */
export function editorLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

/** One control as the controls editor shows it: `question | file`, and
 *  `| source` when it names one. */
export function controlLine(c: Pick<ControlRow, "question" | "expect" | "source">): string {
  return c.source ? `${c.question} | ${c.expect} | ${c.source}` : `${c.question} | ${c.expect}`;
}

export interface ControlDraft {
  question: string;
  expect: string;
  source: string | null;
}

/** The controls editor's text read back: one `question | file | source`
 *  line each (no source is the knowledge base itself). A line without both a
 *  question and a file is returned in `bad`, so nothing half-read is saved. */
export function parseControls(text: string): { controls: ControlDraft[]; bad: string[] } {
  const controls: ControlDraft[] = [];
  const bad: string[] = [];
  for (const line of editorLines(text)) {
    const [question = "", expect = "", source = ""] = line.split("|").map((p) => p.trim());
    if (!question || !expect) {
      bad.push(line);
      continue;
    }
    controls.push({ question, expect, source: source || null });
  }
  return { controls, bad };
}

/** How a control did on its source's last run, in a few words. */
export function controlResult(c: Pick<ControlRow, "ran" | "rank" | "passed">): string {
  if (!c.ran) return "not run yet";
  if (c.passed && c.rank !== null) return `passes, at ${c.rank}`;
  return "fails: not in the top 5";
}
