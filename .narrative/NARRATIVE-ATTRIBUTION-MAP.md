# Narrative Attribution & Session Linking: Architecture Mapping

**Date:** 2026-01-30
**Purpose:** Explain how trace data ingestion, attribution features, and git/AI session linking realize the "narrative version control" design vision.

---

## Executive Summary

The Narrative Desktop app implements **two complementary pipelines** that together realize the "narrative version control" concept from the original design post:

1. **Trace Attribution Pipeline** — Shows *what* AI contributed (line-level attribution)
2. **Session Linking Pipeline** — Shows *why* code changed (conversation-to-commit mapping)

Both pipelines feed into the **Narrative Layer** (Timeline + UI) which presents git history as a readable story rather than raw diffs.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        NARRATIVE VERSION CONTROL                            │
│                    "Treat prompts as first-class citizens"                  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    │                               │
        ┌───────────▼────────────┐      ┌─────────▼─────────────┐
        │  TRACE ATTRIBUTION     │      │  SESSION LINKING       │
        │  (What AI contributed)  │      │  (Why code changed)    │
        └───────────┬────────────┘      └─────────┬─────────────┘
                    │                               │
        ┌───────────▼───────────────────────────────▼────────────┐
        │              SHARED DATABASE LAYER (SQLite)              │
        │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
        │  │trace_records │  │  sessions    │  │session_links │ │
        │  │trace_files   │  │  messages    │  │              │ │
        │  │trace_ranges  │  │              │  │              │ │
        │  └──────────────┘  └──────────────┘  └──────────────┘ │
        └───────────────────────────┬──────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    │   NARRATIVE LAYER (UI)        │
                    │  ┌─────────────────────────┐  │
                    │  │ Timeline (Badges!)      │  │
                    │  │ SessionExcerpts         │  │
                    │  │ AgentTraceSummary       │  │
                    │  │ DiffViewer (Highlights) │  │
                    │  └─────────────────────────┘  │
                    └────────────────────────────────┘
```

---

## Part 1: The Original Design Vision

From the ["Narrative Version Control"](https://twitter.com/kickingkeys/status/1883800695114788133) post, the core ideas are:

### 1.1 Core Problem Statement
> "Traditional version control treats commits as discrete snapshots. You see what changed. You see the diff. **But you do not see a 'why'.** You do not see the conversation that led to the change."

### 1.2 Design Principles
| Principle | Implementation |
|-----------|----------------|
| **Prompts as first-class citizens** | Session excerpts panel shows full conversation |
| **Progressive disclosure** | Timeline → Commit → Files → Diff with line highlights |
| **Multi-level abstraction** | AI% badge at commit level, file pills, line highlights |
| **Intent as entry point** | Non-technical summaries from session messages |

### 1.3 Key UI Concepts from Design
- **Timeline badges** — "Key moments in the LLM sessions" surface on commits
- **AI-Session Highlights** — "Step along the way to the final commit"
- **Intent section** — "Summary of what the engineer was trying to accomplish"
- **Progressive disclosure** — "Top-level timeline → zoom in for more detail"

---

## Part 2: Trace Attribution Pipeline (What AI Contributed)

### 2.1 Data Flow

```
┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
│   INPUT SOURCES   │─────▶│   INGESTION      │─────▶│    STORAGE       │
│                  │      │                  │      │                  │
│ • Codex OTEL     │      │ • otelAdapter.ts  │      │ • trace_records  │
│ • Agent Trace    │      │ • agentTrace.ts   │      │ • trace_files    │
│   JSON files     │      │ • Secret redaction│      │ • trace_ranges   │
│ • .narrative/    │      │                  │      │ • trace_         │
│   trace/         │      │                  │      │   conversations │
└──────────────────┘      └──────────────────┘      └─────────┬────────┘
                                                            │
                                                            ▼
┌──────────────────────────────────────────────────────────────┐
│                        COMPUTATION LAYER                     │
│  • Scan commits for trace records                            │
│  • Compute AI% per commit (aiLines / totalLines)             │
│  • Compute AI% per file (trace_ranges → summary)             │
│  • Extract model IDs (e.g., "claude-4-opus-20250129")        │
└───────────────────────────┬──────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│                         UI SURFACES                          │
│  ┌──────────────────┐  ┌──────────────────┐  ┌─────────────┐ │
│  │ Timeline Badge   │  │ AgentTraceSummary │  │ DiffViewer  │ │
│  │ (AI 73%)         │  │ (AI vs Human)     │  │ (Highlights) │ │
│  └──────────────────┘  └──────────────────┘  └─────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 Implementation Files

| File | Role | Key Functions |
|------|------|---------------|
| `src/core/repo/otelAdapter.ts` | Converts Codex OTEL events → TraceRecord | `otelEnvelopeToCodexEvents()`, `codexOtelEventsToTraceRecords()` |
| `src/core/repo/agentTrace.ts` | Parses/stores Agent Trace JSON | `ingestTraceRecord()`, `scanAgentTraceRecords()`, `getTraceSummaryForCommit()` |
| `src/core/repo/indexer.ts` | Orchestrates trace scanning during repo load | Lines 121-132: trace scanning phase |
| `src-tauri/src/otlp_receiver.rs` | OTLP server for real-time trace collection | Receives OTEL events from Codex CLI, writes to log file |
| `migrations/003_add_agent_trace.sql` | SQLite schema for trace data | Tables: trace_records, trace_files, trace_conversations, trace_ranges |
| `migrations/004_session_attribution.sql` | SQLite schema for session linking | Tables: sessions, session_links, commit_contribution_stats |

### 2.3 Database Schema

#### Migration 003 (Trace Attribution)

```sql
-- Main trace records (one per commit)
CREATE TABLE trace_records (
  id TEXT PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  revision TEXT NOT NULL,          -- Git commit SHA
  version TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  tool_name TEXT,
  metadata_json TEXT
);

-- Files within trace records
CREATE TABLE trace_files (
  id INTEGER PRIMARY KEY,
  record_id TEXT NOT NULL,
  path TEXT NOT NULL               -- File path
);

-- Conversations (who contributed)
CREATE TABLE trace_conversations (
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL,
  contributor_type TEXT,           -- 'ai', 'human', 'mixed', 'unknown'
  model_id TEXT                    -- e.g., 'claude-4-opus-20250129'
);

-- Line ranges (which lines)
CREATE TABLE trace_ranges (
  id INTEGER PRIMARY KEY,
  conversation_id INTEGER NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  contributor_type TEXT,
  model_id TEXT
);
```

#### Migration 004 (Session Linking & Attribution)

```sql
-- Sessions table (stores imported AI coding sessions)
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,              -- Deterministic session ID
    repo_id INTEGER NOT NULL,
    tool TEXT NOT NULL,                -- e.g., 'claude-code', 'cursor'
    model TEXT,                         -- e.g., 'claude-4-opus-20250129'
    checkpoint_kind TEXT DEFAULT 'ai_agent',
    imported_at TEXT NOT NULL,
    duration_min INTEGER,
    message_count INTEGER DEFAULT 0,
    files TEXT,                         -- JSON array of files touched
    raw_json TEXT NOT NULL,             -- Full session trace data
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
);

-- Session-to-commit linking results
CREATE TABLE session_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    session_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    confidence REAL NOT NULL,           -- 0.0 to 1.0 (auto-link threshold: 0.7)
    auto_linked BOOLEAN NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE,
    UNIQUE(repo_id, session_id)
);

-- Pre-computed contribution statistics per commit
CREATE TABLE commit_contribution_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    commit_sha TEXT NOT NULL,
    -- Session-level stats
    ai_agent_lines INTEGER DEFAULT 0,
    ai_assist_lines INTEGER DEFAULT 0,
    human_lines INTEGER DEFAULT 0,
    total_lines INTEGER DEFAULT 0,
    ai_percentage INTEGER DEFAULT 0,    -- 0-100
    -- Metadata
    primary_session_id TEXT,
    tool TEXT,
    model TEXT,
    computed_at TEXT NOT NULL,
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE,
    FOREIGN KEY (primary_session_id) REFERENCES sessions(id) ON DELETE SET NULL,
    UNIQUE(repo_id, commit_sha)
);
```

### 2.4 How It Surfaces in UI

#### Timeline Badge (indexer.ts:139-141)
```typescript
const traceBadge = traceSummary
  ? { type: 'trace' as const, label: `AI ${traceSummary.aiPercent}%` }
  : null;
```

#### AgentTraceSummary Panel
- Shows AI% bar with color coding
- Breaks down lines: AI, Human, Mixed, Unknown
- Lists models used (e.g., "Models: claude-4-opus-20250129")
- Empty state: "No Agent Trace yet"

#### DiffViewer Highlights
```typescript
// diffViewer.tsx:86-90
if (inHunk && traceLookup && !line.startsWith('-')) {
  const traceInfo = traceLookup.get(currentLineNumber);
  if (traceInfo) traceStyle = traceClass(traceInfo.type);
}
```

---

## Part 3: Session Linking Pipeline (Why Code Changed)

### 3.1 Data Flow

```
┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
│   INPUT SOURCES   │─────▶│   LINKING        │─────▶│    STORAGE       │
│                  │      │   ALGORITHM      │      │                  │
│ • Claude Code    │      │                  │      │ • sessions       │
│   session files  │      │ • linking.rs      │      │ • session_links  │
│ • Manual import  │      │   (Rust)          │      │ • confidence     │
│   (JSON/JSONL)   │      │ • Temporal score  │      │ • auto_linked    │
└──────────────────┘      │ • File overlap    │      └──────────────────┘
                          │ • Threshold 0.7  │
                          └──────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│                      COMPUTATION LAYER                        │
│  1. Parse session → extract time window, file paths         │
│  2. Filter commits by time window (±4 hours)                 │
│  3. Score candidates: 0.6 × temporal + 0.4 × file_overlap    │
│  4. Auto-link if confidence ≥ 0.7                            │
└───────────────────────────┬──────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│                         UI SURFACES                          │
│  ┌──────────────────┐  ┌──────────────────┐  ┌─────────────┐ │
│  │ SessionExcerpts  │  │ Link Status      │  │ Timeline    │ │
│  │ Panel           │  │ (Auto, 85%)      │  │ Navigation  │ │
│  └──────────────────┘  └──────────────────┘  └─────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### 3.2 Implementation Files

| File | Role | Key Functions |
|------|------|---------------|
| `src-tauri/src/linking.rs` | Core linking algorithm in Rust | `score_temporal_overlap()`, `score_file_overlap()`, `link_session_to_commits()` |
| `src-tauri/src/import/commands.rs` | Session import Tauri commands | `import_session_file()`, `get_recent_sessions()` |
| `src/core/repo/sessionLinking.ts` | Frontend API wrappers | `linkSessionToCommit()`, `getSessionLinksForCommit()` |
| `src/core/repo/sessions.ts` | Session loading from disk/DB | `loadSessionExcerpts()` |

### 3.3 Linking Algorithm (linking.rs:338-369)

```rust
/// Combined score = 0.6 * temporal + 0.4 * file_overlap
/// Auto-link if confidence >= 0.7 (70%)
pub fn calculate_link_confidence(
    session_end: &DateTime<Utc>,
    session_duration_min: i64,
    commit: &GitCommit,
    session_files: &[String],
) -> Option<LinkResult> {
    let temporal_score = score_temporal_overlap(session_end, session_duration_min, &commit_time);
    let file_score = score_file_overlap(session_files, &commit.files);
    let confidence = (0.6 * temporal_score) + (0.4 * file_score);

    if confidence >= 0.7 {
        Some(LinkResult { commit_sha, confidence, auto_linked: true, temporal_score, file_score })
    } else {
        None  // Below threshold → mark as unlinked
    }
}
```

### 3.4 How It Surfaces in UI

#### SessionExcerpts Panel (SessionExcerpts.tsx:56-111)
- **LinkStatus component** shows:
  - `Linked to abc12345` (short SHA)
  - Confidence badge (e.g., `85%`)
  - `Auto` pill for auto-linked sessions
  - `Unlink` button with confirmation dialog
- **Not linked** state shows `Link2Off` icon when no confident match

#### Timeline Navigation
- Click linked commit SHA → scroll timeline to that commit
- Click timeline node → show linked session excerpts

---

## Part 4: Where Narrative Ideas Surface in the App

### 4.1 "Prompts as First-Class Citizens"

| Design Idea | Implementation | File |
|-------------|----------------|------|
| "See the conversation that led to the change" | SessionExcerpts panel shows full message history | `SessionExcerpts.tsx` |
| "Non-technical explanation" | Intent summaries from commit subjects (placeholder for LLM) | `indexer.ts:99-103` |
| "Key moments in LLM sessions" | Session messages with collapsible text, file pills | `SessionExcerpts.tsx:238-263` |

### 4.2 "Progressive Disclosure"

```
┌─────────────────────────────────────────────────────────────┐
│ Level 1: Timeline (commit list with AI% badges)            │
│   ●───●───●───●───●                                         │
│  AI 73%        AI 45%                                       │
└──────────────────────┬──────────────────────────────────────┘
                       │ Click commit
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Level 2: Commit Detail (session + files + attribution)      │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐          │
│  │ Session     │ │ Files       │ │ AI Summary  │          │
│  │ Excerpts    │ │ Changed     │ │ Agent Trace │          │
│  └─────────────┘ └─────────────┘ └─────────────┘          │
└──────────────────────┬──────────────────────────────────────┘
                       │ Click file
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Level 3: File Diff (line-level AI highlights)              │
│  + This line was AI-generated (green highlight)            │
│  + This line was human-edited (gray highlight)             │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 "Multi-Level Abstraction"

From the design post:
> "The top-level timeline shows key milestones... Zoom in and more is revealed."

| Abstraction Level | UI Component | Data Source |
|-------------------|--------------|-------------|
| **Executive view** | Timeline badges (AI 73%) | `trace.byCommit[sha].aiPercent` |
| **Manager view** | SessionExcerpts (key moments) | `sessionExcerpts[0].messages` |
| **Engineer view** | DiffViewer (line highlights) | `traceRanges` per file |
| **Forensics view** | AgentTraceSummary (models, counts) | `trace.commit.modelIds` |

### 4.4 "Intent as Entry Point"

| Design Quote | Implementation |
|--------------|----------------|
| "Non-technical explanation of what the branch is implementing" | `intent` array from commit subjects (MVP) |
| "Summary of ideas as they were crafted in dialogue" | SessionExcerpts shows user/assistant messages |
| "Whether tests failed" | `testRunId` on TimelineNode (future integration) |

---

## Part 5: Shared Foundations

### 5.1 Common Data Layer

Both pipelines share:
- **SQLite database** (`src-tauri/migrations/`)
- **Repository ID** (`repoId`) for multi-repo support
- **Commit SHA** as primary key for linking
- **File paths** for overlap detection

### 5.2 Shared UI Concepts

| Concept | Used By Trace | Used By Session |
|---------|---------------|-----------------|
| Timeline badges | ✅ (AI 73%) | 🚧 (planned: session badge) |
| File pills | ✅ (AI% per file) | ✅ (files touched in session) |
| Link indicators | N/A | ✅ (Linked to abc12345) |
| Empty states | ✅ (No Agent Trace) | ✅ (No sessions imported) |

### 5.3 Integration Points

The **indexer** (`src/core/repo/indexer.ts`) orchestrates both pipelines:

```typescript
// Phase order showing both pipelines
const phaseOrder = [
  'commits',        // Git: list commits
  'sessions',       // Session: load excerpts
  'trace-config',   // Trace: load config
  'trace',          // Trace: scan records (lines 121-132)
  'meta',           // Write .narrative/meta snapshots
  'done'
];

// Both pipelines contribute to BranchViewModel
const model: BranchViewModel = {
  sessionExcerpts,      // From session pipeline
  traceSummaries: {     // From trace pipeline
    byCommit: trace.byCommit,
    byFileByCommit: trace.byFileByCommit
  },
  timeline: commits.map(c => ({
    badges: traceBadge ? [traceBadge] : undefined  // Trace badges on timeline
  }))
};
```

---

## Part 6: Security & Privacy

Both pipelines implement security measures:

| Pipeline | Security Measure | Implementation |
|----------|------------------|----------------|
| Trace | Secret redaction before ingest | `redactSecrets()` in `agentTrace.ts:269` |
| Trace | No legal ownership claim | UI disclaimer: "helpful guide, not legal ownership" |
| Session | Secret detection on import | `detect_secrets()` in `linking.rs:412-459` |
| Session | Path traversal protection | `normalize_path()` in `linking.rs:280-310` |

---

## Part 7: Evidence Map (Spec → Implementation)

### 7.1 Trace Attribution

| Spec Source | Implementation | Status |
|-------------|----------------|--------|
| `build-plan-2026-01-29-agent-trace-integration.md` Epic 1 | Migration 003 schema | ✅ Complete |
| `build-plan-2026-01-29-agent-trace-integration.md` Epic 2 | `agentTrace.ts` ingest | ✅ Complete |
| `build-plan-2026-01-29-agent-trace-integration.md` Epic 3 | Timeline badge, file pills | ✅ Complete |
| `build-plan-2026-01-29-agent-trace-integration.md` Epic 4 | Diff line highlights | ✅ Complete |
| `build-plan-2026-01-29-agent-trace-integration.md` Epic 5 | Export derived trace | ✅ Complete |

### 7.2 Session Linking

| Spec Source | Implementation | Status |
|-------------|----------------|--------|
| `prd-2026-01-29-session-commit-linking.md` FR1 | Import with linking | ✅ Complete |
| `prd-2026-01-29-session-commit-linking.md` FR2 | Timeline badge | 🚧 Uses trace badge (unified planned) |
| `prd-2026-01-29-session-commit-linking.md` FR3 | Session panel state | ✅ Complete |
| `prd-2026-01-29-session-commit-linking.md` FR4 | Unlink flow | ✅ Complete |

### 7.3 Narrative Design Concepts

| Design Concept | Implementation | Status |
|----------------|----------------|--------|
| "Prompts as first-class citizens" | SessionExcerpts panel | ✅ Complete |
| "Progressive disclosure" | Timeline → Commit → File → Diff | ✅ Complete |
| "Multi-level abstraction" | AI% badge → summary → line highlights | ✅ Complete |
| "Intent as entry point" | Commit summaries (LLM future) | 🚧 MVP uses commit subject |
| "Context beyond code" | Session messages + trace ranges | ✅ Complete |

---

## Part 8: File Reference Map

### Core Pipeline Files

```
src/core/repo/
├── indexer.ts           # Main orchestrator (lines 105-132: trace & session)
├── agentTrace.ts        # Trace ingest + scan + summary computation
├── otelAdapter.ts       # Codex OTEL → TraceRecord conversion
├── traceConfig.ts       # Configuration for trace collector
├── sessionLinking.ts    # Frontend API for session linking
├── sessions.ts          # Session loading from disk/DB
├── git.ts              # Git operations (listCommits, getCommitDetails)
└── db.ts               # SQLite access layer

src-tauri/src/
├── linking.rs           # Session→commit linking algorithm (Rust)
├── import/
│   ├── parser.rs        # Session JSON parsing
│   └── commands.rs      # Session import Tauri commands
└── attribution/
    ├── models.rs        # Attribution data types
    └── commands.rs      # Attribution stats commands

src-tauri/migrations/
├── 003_add_agent_trace.sql        # Trace tables
└── 004_session_attribution.sql    # Session + stats tables
```

### UI Files

```
src/ui/
├── views/
│   └── BranchView.tsx     # Main layout (timeline + panels)
└── components/
    ├── Timeline.tsx        # Timeline with badges
    ├── SessionExcerpts.tsx # Session panel + link status
    ├── AgentTraceSummary.tsx # AI vs Human summary
    ├── FilesChanged.tsx    # File list with AI% pills
    ├── DiffViewer.tsx      # Diff with line highlights
    └── TraceTranscriptPanel.tsx # Full conversation view
```

---

## Part 9: Future Directions

### 9.1 Near-Term (Spec'd but not implemented)

| Feature | Spec | Status |
|---------|------|--------|
| Manual link creation | `prd-2026-01-29-session-commit-linking.md` US3.2 | 🚧 v2 |
| Multi-commit linking | `prd-2026-01-29-session-commit-linking.md` | 🚧 v2 |
| Git trailer parsing | `prd-2026-01-29-session-commit-linking.md` | 🚧 v2 |
| Export to .narrative/meta/ | `prd-2026-01-29-session-commit-linking.md` | 🚧 v2 |

### 9.2 Longer-Term (from design post)

| Feature | Design Quote | Implementation Gap |
|---------|--------------|-------------------|
| "Narrative foresight" | "Plot a change and see what it might mean" | Not specced |
| "Context beyond code" | "Meeting transcripts, chat histories" | Not specced |
| LLM summaries | "Smaller LLM to extract summaries and intent" | Not specced |
| Real-time streaming | "Session as it happens" | Not specced |

---

## Part 10: Testing & Validation

### 10.1 Unit Tests

| Component | Test File | Coverage | Status |
|-----------|-----------|----------|--------|
| OTEL adapter parsing | `src/core/repo/__tests__/otelAdapter.test.ts` | Event envelope parsing, record conversion | ✅ |
| AgentTraceSummary UI | `src/ui/components/__tests__/AgentTraceSummary.test.tsx` | Badge display, empty states, status indicators | ✅ |
| Linking algorithm | `src-tauri/src/linking.rs` (lines 621-708) | Temporal overlap, file overlap, threshold logic | ✅ |

### 10.2 Integration Tests

| Scenario | Test Approach | Status |
|----------|---------------|--------|
| Session import → linking → SQLite | Manual: Import session file, verify `session_links` entry | ✅ Documented in ATTRIBUTION-FINAL-SUMMARY |
| Trace scan → badge → timeline | Manual: Scan `.narrative/trace/`, verify timeline badges appear | ✅ Documented in build-plan |
| OTLP ingest → database | Smoke test: `runOtlpSmokeTest()` command | ✅ Implemented in UI |

### 10.3 Validation Commands

```bash
# Run Rust tests (including linking algorithm)
cd src-tauri
cargo test

# Verify trace scan functionality
# 1. Place test trace file in .narrative/trace/
# 2. Run: pnpm tauri dev
# 3. Open repo and verify badges appear

# Verify session linking
# 1. Import session via UI or API
# 2. Check confidence score in SessionExcerpts panel
# 3. Verify auto-link at ≥70% confidence
```

### 10.4 Performance Metrics

From `ATTRIBUTION-FINAL-SUMMARY.md`:

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Single import | < 500ms | ~100ms | ✅ |
| Timeline query | < 100ms | ~20ms (cached) | ✅ |
| Batch (10 files) | < 2s | ~1s | ✅ |
| Secret scan | N/A | ~1ms per KB | ✅ |

---

## Summary

The Narrative Desktop app realizes the **"narrative version control"** vision through two complementary pipelines:

1. **Trace Attribution** answers "What did AI contribute?" with line-level precision
2. **Session Linking** answers "Why did code change?" by connecting conversations to commits

Both feed into the **Narrative Layer** (Timeline + panels) which provides:
- **Progressive disclosure** (Timeline → Commit → File → Diff)
- **Multi-level abstraction** (AI% badges → summaries → line highlights)
- **Intent as entry point** (Session excerpts explain the "why")

The architecture is **layered** (input → ingest → storage → compute → UI) with **shared foundations** (SQLite, repoId, commit SHA) and **consistent security** (secret redaction, path validation).

---

**Document Version:** 1.0
**Last Updated:** 2026-01-30
**Related Specs:**
- `.spec/build-plan-2026-01-29-agent-trace-integration.md`
- `.spec/foundation-2026-01-29-agent-trace-integration.md`
- `.spec/prd-2026-01-29-session-commit-linking.md`
- `.spec/ATTRIBUTION-FINAL-SUMMARY.md`
