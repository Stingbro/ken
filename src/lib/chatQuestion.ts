// AskUserQuestion payloads: the JSON carried by a "question" chat message.
// Parsing is deliberately tolerant — anything we can't make sense of returns
// null and the transcript renders nothing rather than a broken card.

export interface QuestionOption {
  label: string;
  description?: string;
  preview?: string;
}

export interface ChatQuestion {
  question: string;
  /** Short chip label (≤12 chars); may be empty. */
  header: string;
  multiSelect: boolean;
  options: QuestionOption[];
}

export interface QuestionPayload {
  requestId: string;
  toolUseId: string;
  questions: ChatQuestion[];
  /** question text → answer string, or null while pending. */
  answers: Record<string, string> | null;
}

/** One question's in-progress selection: chosen labels plus free-text. */
export interface QuestionSelection {
  labels: string[];
  other?: string;
}

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}

function parseOption(raw: unknown): QuestionOption | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const label = str(o.label);
  if (!label) return null;
  const out: QuestionOption = { label };
  if (typeof o.description === "string") out.description = o.description;
  if (typeof o.preview === "string") out.preview = o.preview;
  return out;
}

function parseQuestion(raw: unknown): ChatQuestion | null {
  if (!raw || typeof raw !== "object") return null;
  const q = raw as Record<string, unknown>;
  const question = str(q.question);
  if (!question) return null;
  const options = Array.isArray(q.options)
    ? q.options.map(parseOption).filter((o): o is QuestionOption => o !== null)
    : [];
  if (options.length === 0) return null;
  return { question, header: str(q.header), multiSelect: q.multiSelect === true, options };
}

function parseAnswers(raw: unknown): Record<string, string> | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof v === "string") out[k] = v;
  }
  return out;
}

export function parseQuestionPayload(content: string): QuestionPayload | null {
  let raw: unknown;
  try {
    raw = JSON.parse(content);
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const p = raw as Record<string, unknown>;
  if (!Array.isArray(p.questions) || p.questions.length === 0) return null;
  const questions: ChatQuestion[] = [];
  for (const q of p.questions) {
    const parsed = parseQuestion(q);
    if (!parsed) return null;
    questions.push(parsed);
  }
  return {
    requestId: str(p.requestId),
    toolUseId: str(p.toolUseId),
    questions,
    answers: parseAnswers(p.answers),
  };
}

export function isAnswered(p: QuestionPayload): boolean {
  const a = p.answers;
  if (!a) return false;
  return p.questions.every((q) => (a[q.question] ?? "").trim().length > 0);
}

/** Collapse in-progress selections into the wire answer map, or null while any
 *  question is still unanswered. Free text wins over checked labels. */
export function buildAnswers(
  p: QuestionPayload,
  selections: Map<string, QuestionSelection>,
): Record<string, string> | null {
  const out: Record<string, string> = {};
  for (const q of p.questions) {
    const sel = selections.get(q.question);
    if (!sel) return null;
    const other = (sel.other ?? "").trim();
    const answer = other || sel.labels.join(", ");
    if (!answer) return null;
    out[q.question] = answer;
  }
  return out;
}

/** "Header: answer" lines for the answered card. */
export function answerSummary(p: QuestionPayload): string[] {
  if (!isAnswered(p)) return [];
  const a = p.answers!;
  return p.questions.map((q) => `${q.header || q.question}: ${a[q.question]}`);
}
