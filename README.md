# Narrative Desktop MVP (tauri v2 + React + Vite + Tailwind v4 + TS)

Desktop MVP for exploring **version control as a narrative medium**.

Includes:

- **Demo mode** mirroring the “branch narrative” layout (intent → session excerpts → files → diff → timeline).
- **repo mode**:
  - pick a local git repo
  - index recent commits
  - timeline navigation + per-commit files/diff on selection
  - caches commit metadata + file-change lists in **SQLite**
  - writes **committable** narrative metadata into `.narrative/meta/**`
  - supports **manual session import** into `.narrative/sessions/**` (with basic secret scrubbing)

---

## prereqs

- Node.js + pnpm
- Rust toolchain
- tauri system deps for your OS (see tauri docs)
- `git` available on PATH (repo mode executes `git` via `tauri-plugin-shell`)

---

## Quick Start

1. **Install dependencies**:

   ```bash
   pnpm install
   ```

2. **Run the app**:

   ```bash
   pnpm tauri dev
   ```

3. **Open a repository**:
   - Click "Open repo..." and select a git repository
   - Navigate commits and see files changed

4. **Import a session** (optional):
   - Click "Import session..." and select a JSON session file
   - See session excerpts in the right panel

## Docs

- `docs/README.md` — docs index (start here)
- `docs/agents/development.md` — dev prerequisites + run/build commands
- `docs/agents/testing.md` — lint, Typecheck, and test commands
- `docs/agents/tauri.md` — tauri permissions + data locations
- `docs/agents/repo-structure.md` — repo layout overview
- `pnpm docs:lint` — run Vale + markdownlint for docs

---

## Run (dev)

```bash
pnpm install
pnpm tauri dev
```

---

## tauri v2 permissions (important)

tauri v2 requires explicit permissions for plugins.

This MVP enables (see `src-tauri/capabilities/default.json`):

- `dialog:allow-open` — choose a repo folder / import a session file
- `shell:allow-execute` — scoped to only the `git` program
- `sql:default` + `sql:allow-execute` — SQLite caching

---

## What gets written to your repo

When you open a repo, the app creates:

- `.narrative/meta/repo.json`
- `.narrative/meta/branches/<branch>.json`
- `.narrative/meta/commits/<sha>.json` for indexed commits
- (lazy) `.narrative/meta/commits/<sha>.files.json` once you click a commit

These files are **committable** (shareable narrative layer).

---

## Sessions

Use **Import session…** (repo tab) to import a JSON file.

Example schema: `examples/session.example.json`.

- The importer performs basic secret redaction before writing into `.narrative/sessions/imported/*`.
- The UI will show the latest imported session in the “Session Excerpts” panel (MVP: branch-level, not commit-linked yet).

---

## Local cache

SQLite file: `sqlite:narrative.db` (managed by `tauri-plugin-sql`)

Schema/migration: `src-tauri/migrations/001_init.sql`

Do **not** commit this cache file.

---

## Next expansions

- Parse Claude Code / Codex CLI native logs and auto-link sessions to commits.
- Commit–session linking via trailers, git notes, and timestamp+file overlap heuristics.
- Multi-level abstraction: commit → session → milestone → branch narrative.
- “Speculate” mode: simulate alternative future paths using constraints learned from history.

---

<img
  src="./brand/brand-mark.webp"
  srcset="./brand/brand-mark.webp 1x, ./brand/brand-mark@2x.webp 2x"
  alt="brAInwav"
  height="28"
  align="left"
/>

<br clear="left" />

**brAInwav**
_from demo to duty_
