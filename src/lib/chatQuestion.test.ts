import { describe, expect, it } from "vitest";
import {
  answerSummary,
  buildAnswers,
  isAnswered,
  parseQuestionPayload,
  type QuestionPayload,
} from "./chatQuestion";

const single: QuestionPayload = {
  requestId: "r1",
  toolUseId: "t1",
  questions: [
    {
      question: "Which auth method?",
      header: "Auth",
      multiSelect: false,
      options: [
        { label: "OAuth", description: "Delegated" },
        { label: "API key", description: "Simple" },
      ],
    },
  ],
  answers: null,
};

const two: QuestionPayload = {
  requestId: "r2",
  toolUseId: "t2",
  questions: [
    {
      question: "Which auth method?",
      header: "Auth",
      multiSelect: false,
      options: [{ label: "OAuth" }, { label: "API key" }],
    },
    {
      question: "Which stores?",
      header: "Stores",
      multiSelect: true,
      options: [{ label: "Postgres" }, { label: "Redis" }, { label: "S3" }],
    },
  ],
  answers: null,
};

describe("parseQuestionPayload", () => {
  it("parses a well-formed payload", () => {
    const p = parseQuestionPayload(JSON.stringify(single));
    expect(p).not.toBeNull();
    expect(p!.requestId).toBe("r1");
    expect(p!.questions).toHaveLength(1);
    expect(p!.questions[0].options[0].label).toBe("OAuth");
    expect(p!.questions[0].multiSelect).toBe(false);
    expect(p!.answers).toBeNull();
  });

  it("parses an answered payload", () => {
    const p = parseQuestionPayload(
      JSON.stringify({ ...single, answers: { "Which auth method?": "OAuth" } }),
    );
    expect(p!.answers).toEqual({ "Which auth method?": "OAuth" });
  });

  it("returns null for malformed JSON", () => {
    expect(parseQuestionPayload("not json")).toBeNull();
    expect(parseQuestionPayload("")).toBeNull();
  });

  it("returns null for non-object / array JSON", () => {
    expect(parseQuestionPayload("42")).toBeNull();
    expect(parseQuestionPayload("null")).toBeNull();
    expect(parseQuestionPayload("[]")).toBeNull();
  });

  it("returns null when questions are missing or empty", () => {
    expect(parseQuestionPayload(JSON.stringify({ requestId: "r" }))).toBeNull();
    expect(
      parseQuestionPayload(JSON.stringify({ requestId: "r", questions: [] })),
    ).toBeNull();
  });

  it("returns null when a question lacks text or options", () => {
    expect(
      parseQuestionPayload(
        JSON.stringify({ questions: [{ header: "H", options: [{ label: "A" }] }] }),
      ),
    ).toBeNull();
    expect(
      parseQuestionPayload(JSON.stringify({ questions: [{ question: "Q", options: [] }] })),
    ).toBeNull();
  });

  it("drops options without a label and rejects the payload if none remain", () => {
    expect(
      parseQuestionPayload(
        JSON.stringify({ questions: [{ question: "Q", options: [{ description: "x" }] }] }),
      ),
    ).toBeNull();
  });

  it("defaults missing header/multiSelect/answers", () => {
    const p = parseQuestionPayload(
      JSON.stringify({ questions: [{ question: "Q", options: [{ label: "A" }] }] }),
    );
    expect(p!.questions[0].header).toBe("");
    expect(p!.questions[0].multiSelect).toBe(false);
    expect(p!.answers).toBeNull();
    expect(p!.requestId).toBe("");
  });

  it("keeps option descriptions and previews when present", () => {
    const p = parseQuestionPayload(
      JSON.stringify({
        questions: [
          { question: "Q", options: [{ label: "A", description: "d", preview: "p" }] },
        ],
      }),
    );
    expect(p!.questions[0].options[0]).toEqual({
      label: "A",
      description: "d",
      preview: "p",
    });
  });

  it("ignores an answers value that is not an object of strings", () => {
    const p = parseQuestionPayload(
      JSON.stringify({ questions: [{ question: "Q", options: [{ label: "A" }] }], answers: 5 }),
    );
    expect(p!.answers).toBeNull();
  });
});

describe("isAnswered", () => {
  it("is false with no answers", () => {
    expect(isAnswered(single)).toBe(false);
    expect(isAnswered({ ...single, answers: {} })).toBe(false);
  });

  it("is false until every question is answered", () => {
    expect(isAnswered({ ...two, answers: { "Which auth method?": "OAuth" } })).toBe(false);
  });

  it("is true when every question has a non-empty answer", () => {
    expect(
      isAnswered({
        ...two,
        answers: { "Which auth method?": "OAuth", "Which stores?": "Redis, S3" },
      }),
    ).toBe(true);
  });

  it("treats a blank answer as unanswered", () => {
    expect(isAnswered({ ...single, answers: { "Which auth method?": "  " } })).toBe(false);
  });
});

describe("buildAnswers", () => {
  it("returns null when nothing is selected", () => {
    expect(buildAnswers(single, new Map())).toBeNull();
  });

  it("builds a single-select answer", () => {
    expect(buildAnswers(single, new Map([["Which auth method?", { labels: ["OAuth"] }]]))).toEqual({
      "Which auth method?": "OAuth",
    });
  });

  it("joins multiSelect labels with a comma", () => {
    const m = new Map([
      ["Which auth method?", { labels: ["OAuth"] }],
      ["Which stores?", { labels: ["Postgres", "S3"] }],
    ]);
    expect(buildAnswers(two, m)).toEqual({
      "Which auth method?": "OAuth",
      "Which stores?": "Postgres, S3",
    });
  });

  it("returns null while any question is unanswered", () => {
    expect(buildAnswers(two, new Map([["Which stores?", { labels: ["S3"] }]]))).toBeNull();
  });

  it("lets a non-empty other override the labels", () => {
    const m = new Map([
      ["Which auth method?", { labels: ["OAuth"], other: "mTLS" }],
    ]);
    expect(buildAnswers(single, m)).toEqual({ "Which auth method?": "mTLS" });
  });

  it("ignores a blank other and falls back to labels", () => {
    const m = new Map([["Which auth method?", { labels: ["OAuth"], other: "   " }]]);
    expect(buildAnswers(single, m)).toEqual({ "Which auth method?": "OAuth" });
  });

  it("accepts other alone with no labels", () => {
    const m = new Map([["Which auth method?", { labels: [], other: "mTLS" }]]);
    expect(buildAnswers(single, m)).toEqual({ "Which auth method?": "mTLS" });
  });

  it("returns null when a selection has neither labels nor other", () => {
    expect(buildAnswers(single, new Map([["Which auth method?", { labels: [] }]]))).toBeNull();
  });

  it("trims the other text", () => {
    const m = new Map([["Which auth method?", { labels: [], other: " mTLS " }]]);
    expect(buildAnswers(single, m)).toEqual({ "Which auth method?": "mTLS" });
  });
});

describe("answerSummary", () => {
  it("is empty when unanswered", () => {
    expect(answerSummary(single)).toEqual([]);
  });

  it("renders Header: answer per question", () => {
    expect(
      answerSummary({
        ...two,
        answers: { "Which auth method?": "OAuth", "Which stores?": "Redis, S3" },
      }),
    ).toEqual(["Auth: OAuth", "Stores: Redis, S3"]);
  });

  it("falls back to the question text when there is no header", () => {
    const p: QuestionPayload = {
      ...single,
      questions: [{ ...single.questions[0], header: "" }],
      answers: { "Which auth method?": "OAuth" },
    };
    expect(answerSummary(p)).toEqual(["Which auth method?: OAuth"]);
  });
});
