# Common Docs — Documentation Guidelines

Follow the Automatic documentation specification when creating or modifying files in the `docs/` directory. These rules apply to all documentation changes.

## Structure

- All documentation lives in `docs/` and follows the standard section layout (context, architecture, adr, plans, api, configuration, integrations, security, guides, operations, migrations, changelog).
- Every directory must have an `index.md` that lists and describes all files within it.
- One concern per file. If a file exceeds ~300 lines, split it.
- File names use `kebab-case`, lowercase. No spaces, no underscores except in ADR/migration prefixes.

## Frontmatter

Every `.md` file in `docs/` must open with YAML frontmatter:

```yaml
---
title: Short descriptive title
description: One sentence describing what this document contains.
status: draft | active | deprecated | superseded
updated: YYYY-MM-DD
authors:
  - Name
related:
  - path/to/related.md
---
```

## Content Rules

- Plans describe the future. ADRs record the past. Architecture describes the present. Never conflate these.
- ADRs are immutable once accepted. To change a decision, write a new ADR that supersedes it.
- ADR numbering is sequential and never reused.
- `docs/` is versioned alongside code, not a wiki. Stale documentation must be updated or deleted.
- Every PR that changes behaviour should update the relevant `docs/` files.
- No actual secret values in documentation. Reference where secrets are stored instead.

## Navigation

- `docs/index.md` is the master entry point. Keep it current.
- `AGENTS.md` or `CLAUDE.md` at the repo root must reference `docs/index.md` as the starting point.
- Use the `common-docs-find` skill to locate the correct file for any topic.
- Use the `common-docs-scaffold` skill to scaffold new sections.
