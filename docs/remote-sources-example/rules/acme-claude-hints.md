# Acme Claude-Specific Guidance

This rule is only loaded for Claude Code via agent_overrides.

## Acme CLI

Claude Code has access to the `acme` CLI. Use it for:

- `acme status` — check service health
- `acme logs --tail` — stream recent logs
- `acme db migrate:status` — check pending migrations
- `acme test:affected` — run tests for changed files only

## Memory

When you discover Acme-specific conventions or quirks during a session, store them using the Automatic memory tools so they persist across sessions.

## Commit Messages

Acme uses conventional commits with scope:

```
feat(billing): add invoice PDF generation
fix(auth): handle expired refresh tokens
chore(deps): bump @acme/ui to 4.2.0
```
