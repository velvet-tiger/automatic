---
name: automatic-documentation
description: Principles for writing READMEs, API docs, ADRs, code comments, and changelogs. Use when creating or improving any technical documentation.
authors:
  - Automatic
---

# Technical Documentation

Documentation is a product. It requires the same deliberateness as code. Outdated, incomplete, or misleading documentation is worse than none — it wastes time and creates false confidence.

## Principles

**Write for the reader, not the writer.** The author already knows how the system works. Documentation exists to transfer that understanding to someone who does not. Every sentence should serve that goal.

**Document decisions, not just descriptions.** Code describes what the system does. Documentation should explain *why* it does it that way. The most valuable documentation captures constraints, trade-offs, and rejected alternatives.

**Treat documentation as a deliverable.** A feature is not complete until it is documented. Documentation that is written later rarely gets written.

---

## Types of Documentation

### README
The entry point for any project. Must answer:
- What does this project do?
- How do I get it running locally?
- How do I run the tests?
- Where do I go for more information?

Keep it short. Link to deeper documentation. It should take under 5 minutes to read.

### API documentation
Document every public interface: functions, REST endpoints, event schemas.

For each item, include:
- **Purpose** — what it does in one sentence
- **Parameters / fields** — name, type, whether required, valid values or range, default
- **Return value / response** — what is returned and under what conditions
- **Errors** — what error conditions can occur and what they mean
- **Example** — a concrete, working example

### Architecture decision records (ADRs)
Record significant technical decisions as short, dated documents:

```markdown
# ADR-042: Use PostgreSQL for primary data store

**Status**: Accepted  
**Date**: 2025-03-02

## Context
We needed a relational database that supports ...

## Decision
We will use PostgreSQL 16 because ...

## Consequences
- We gain: JSONB support, strong ACID guarantees, mature tooling
- We accept: operational complexity vs. a managed service
- We reject: MySQL (weaker JSON support), SQLite (no concurrent writes)
```

ADRs are permanent. Even superseded decisions should be marked "Superseded" and kept — the reasoning matters.

### Code comments
Comment *why*, not *what*. The code says what is happening; comments explain why it happens that way.

```
// Good: "Skip validation here — this path is only reachable from the
//        internal job queue, which has already validated the payload."

// Bad: "Skip validation"
// Bad: "Call the validate function" (the next line says that)
```

Comment anything surprising, non-obvious, or that required a decision that is not evident from the code itself.

### Changelogs
Record what changed for users, not for developers.

- Group by release version and date
- Categorise: Added, Changed, Deprecated, Removed, Fixed, Security
- Link to issues or PRs for context
- Never use "Various bug fixes" — be specific about what was fixed

---

## Writing Style

**Use plain English.** Avoid jargon where a simpler word exists. If jargon is necessary, define it on first use.

**Short sentences.** Break long sentences into two. If a sentence requires re-reading, rewrite it.

**Active voice.** "The server validates the token" not "The token is validated by the server."

**Present tense.** "Returns the user object" not "Will return the user object."

**Examples are mandatory.** Abstract descriptions without examples make readers do extra work. Show, then tell.

---

## Maintenance

- Documentation lives alongside the code it describes — in the same repository
- Documentation changes are part of the same PR as the code changes
- Broken documentation is a bug — file it and fix it
- Deprecate documented features explicitly; do not silently remove documentation
