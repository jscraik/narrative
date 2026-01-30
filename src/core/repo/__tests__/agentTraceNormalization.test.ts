import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { scanAgentTraceRecords, getTraceRangesForCommitFile } from "../agentTrace";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("../../tauri/narrativeFs", () => ({
  listNarrativeFiles: vi.fn().mockResolvedValue([]),
  readNarrativeFile: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

describe("scanAgentTraceRecords normalization", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("normalizes snake_case summary payloads from Rust into camelCase TraceCommitSummary", async () => {
    // Mock response with snake_case keys (legacy Rust without #[serde(rename_all)])
    const snakeCasePayload = {
      "abc123": {
        commit: {
          commit_sha: "abc123",
          ai_lines: 100,
          human_lines: 50,
          mixed_lines: 10,
          unknown_lines: 5,
          ai_percent: 60,
          model_ids: ["claude-4-opus"],
          tool_names: ["claude_code"],
        },
        files: {},
        totals: {
          conversations: 5,
          ranges: 20,
        },
      },
    };

    mockInvoke.mockResolvedValue(snakeCasePayload);

    const result = await scanAgentTraceRecords("/fake/repo", 1, ["abc123"]);

    expect(result.byCommit.abc123).toMatchInlineSnapshot(`
      {
        "aiLines": 100,
        "aiPercent": 60,
        "commitSha": "abc123",
        "humanLines": 50,
        "mixedLines": 10,
        "modelIds": [
          "claude-4-opus",
        ],
        "toolNames": [
          "claude_code",
        ],
        "unknownLines": 5,
      }
    `);
  });

  it("normalizes camelCase summary payloads from Rust", async () => {
    // Mock response with camelCase keys (new Rust with #[serde(rename_all = "camelCase")])
    const camelCasePayload = {
      "def456": {
        commit: {
          commitSha: "def456",
          aiLines: 200,
          humanLines: 100,
          mixedLines: 20,
          unknownLines: 10,
          aiPercent: 60,
          modelIds: ["gpt-4o"],
          toolNames: ["cursor"],
        },
        files: {},
        totals: {
          conversations: 10,
          ranges: 40,
        },
      },
    };

    mockInvoke.mockResolvedValue(camelCasePayload);

    const result = await scanAgentTraceRecords("/fake/repo", 1, ["def456"]);

    expect(result.byCommit.def456).toEqual({
      commitSha: "def456",
      aiLines: 200,
      humanLines: 100,
      mixedLines: 20,
      unknownLines: 10,
      aiPercent: 60,
      modelIds: ["gpt-4o"],
      toolNames: ["cursor"],
    });
  });

  it("handles missing toolNames field gracefully", async () => {
    const payloadWithoutToolNames = {
      "ghi789": {
        commit: {
          commit_sha: "ghi789",
          ai_lines: 50,
          human_lines: 25,
          mixed_lines: 5,
          unknown_lines: 2,
          ai_percent: 60,
          model_ids: [],
          // tool_names missing
        },
        files: {},
        totals: {
          conversations: 2,
          ranges: 8,
        },
      },
    };

    mockInvoke.mockResolvedValue(payloadWithoutToolNames);

    const result = await scanAgentTraceRecords("/fake/repo", 1, ["ghi789"]);

    expect(result.byCommit.ghi789.toolNames).toEqual([]);
  });
});

describe("getTraceRangesForCommitFile normalization", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("maps snake_case range payload keys to camelCase TraceRange", async () => {
    const snakeCaseRanges = [
      {
        start_line: 10,
        end_line: 20,
        content_hash: "abc123",
        contributor: {
          contributor_type: "ai",
          model_id: "claude-4-opus",
        },
      },
      {
        start_line: 30,
        end_line: 40,
        content_hash: null,
        contributor: {
          contributor_type: "human",
          model_id: null,
        },
      },
    ];

    mockInvoke.mockResolvedValue(snakeCaseRanges);

    const result = await getTraceRangesForCommitFile(1, "abc123", "src/file.ts");

    expect(result).toHaveLength(2);
    expect(result[0]).toMatchInlineSnapshot(`
      {
        "contentHash": "abc123",
        "contributor": {
          "modelId": "claude-4-opus",
          "type": "ai",
        },
        "endLine": 20,
        "startLine": 10,
      }
    `);
    expect(result[1]).toEqual({
      startLine: 30,
      endLine: 40,
      contentHash: undefined,
      contributor: {
        type: "human",
        modelId: undefined,
      },
    });
  });

  it("maps camelCase range payload keys to TraceRange", async () => {
    const camelCaseRanges = [
      {
        startLine: 15,
        endLine: 25,
        contentHash: "def456",
        contributor: {
          contributorType: "mixed",
          modelId: "gpt-4o",
        },
      },
    ];

    mockInvoke.mockResolvedValue(camelCaseRanges);

    const result = await getTraceRangesForCommitFile(1, "def456", "src/other.ts");

    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      startLine: 15,
      endLine: 25,
      contentHash: "def456",
      contributor: {
        type: "mixed",
        modelId: "gpt-4o",
      },
    });
  });

  it("handles missing optional modelId gracefully", async () => {
    const rangesWithoutModelId = [
      {
        start_line: 5,
        end_line: 10,
        content_hash: null,
        contributor: {
          contributor_type: "unknown",
          model_id: null,
        },
      },
    ];

    mockInvoke.mockResolvedValue(rangesWithoutModelId);

    const result = await getTraceRangesForCommitFile(1, "xyz789", "src/unknown.ts");

    expect(result).toHaveLength(1);
    const [range] = result;
    expect(range?.contributor?.modelId).toBeUndefined();
  });
});