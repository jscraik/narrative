# Docs Index

Short index of the repo’s documentation surfaces.

## Scope

- Covers documentation under `docs/`.
- Focuses on developer setup and operational notes for the Narrative desktop app.

## Audience

- Contributors and maintainers running, testing, or releasing the app.

## Contents

- `docs/agents/development.md` — dev prerequisites + run/build commands
- `docs/agents/testing.md` — lint, Typecheck, and test commands
- `docs/agents/tauri.md` — tauri permissions + data locations
- `docs/agents/repo-structure.md` — repo layout overview

## Doc tooling

- Vale config: `.vale.ini`
- Vale styles: `styles/`
- markdownlint config: `.markdownlint-cli2.yaml`
- Brand assets: `brand/` (see `brand/README.md`)

### Run checks (local)

```bash
pnpm docs:lint
```

Manual commands:

```bash
vale --minAlertLevel=warning README.md docs/README.md docs/agents/*.md
npx -y markdownlint-cli2 README.md docs/**/*.md brand/README.md
```

## Maintenance

- Update this index and the README “Docs” section when adding or moving docs.

## Verify

- Ensure each linked file exists and matches the repo layout.

## Meta

- Owner: repo maintainers (update as needed)
- Last updated: 2026-01-30
