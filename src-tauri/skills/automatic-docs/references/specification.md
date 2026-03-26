# Automatic Docs Specification — File Content Reference

Use this as the content template for each stub file when scaffolding.
Replace all `TODO` placeholders with project-specific content when known, otherwise leave them for the user to fill in.

---

## docs/index.md

```markdown
# Project Documentation

> One-paragraph summary: what this project is, what it does, who uses it.

## Sections

| Section | Path | Contains |
|---------|------|----------|
| Domain & product context | `docs/context/` | Terminology, goals, stakeholders |
| System design | `docs/architecture/` | Current architecture, data model, infrastructure |
| Decision log | `docs/adr/` | Immutable record of architectural decisions |
| Future work | `docs/plans/` | Roadmap, epics, feature specs |
| API reference | `docs/api/` | Endpoints, auth, errors |
| Configuration | `docs/configuration/` | Env vars, flags, secrets reference |
| Integrations | `docs/integrations/` | Third-party system docs |
| Security | `docs/security/` | Threat model, auth design, data handling |
| How-to guides | `docs/guides/` | Setup, development, deployment, contributing |
| Production ops | `docs/operations/` | Monitoring, runbooks, disaster recovery |
| Migration log | `docs/migrations/` | DB and data migration history |
| Release history | `docs/changelog/` | What changed in each release |

## Quick Reference

- **Run locally:** TODO
- **Run tests:** TODO
- **Deploy:** see `guides/deployment.md`
- **Key concepts:** see `context/domain.md`
- **Architecture decisions:** see `adr/index.md`
```

---

## context/

### index.md

Explain why this section exists. List all files with one-line descriptions. Note that it should be kept updated as the product evolves.

### product.md

Must contain: Problem statement, target users and their jobs-to-be-done, value proposition, success metrics, out of scope.

### domain.md

The ubiquitous language document. Must contain: alphabetical glossary of domain terms, key concepts and how they relate, similar-but-distinct terms, any external standards the domain is based on.

### stakeholders.md

Must contain: stakeholder roles (not names), what each cares about, who makes which kinds of decisions.

---

## architecture/

### index.md

Note that this section describes **current state**, not future plans (see `plans/`). Include date of last architecture review. List all files with one-line descriptions.

### overview.md

Must contain: a system diagram (Mermaid preferred), a narrative walkthrough, key system properties (e.g. stateless, event-driven, multi-tenant).

### data-model.md

Must contain: core entities and their relationships, storage technology decisions and rationale.

### api-design.md

Must contain: API style (REST/GraphQL/gRPC), versioning strategy, authentication pattern.

### infrastructure.md

Must contain: environment table (production/staging/dev), cloud resources, networking topology.

### services.md

Table of services with: name, responsibility, interface, dependencies.

### constraints.md

Must contain: technical debt with impact assessment, hard scale limits, external constraints (regulatory, contractual, platform).

---

## adr/

### index.md

A table of all ADRs:

| ID | Title | Status | Date |
|----|-------|--------|------|

Statuses: `proposed` | `accepted` | `deprecated` | `superseded`

### template.md

```markdown
---
title: "ADR-NNNN: Title"
status: proposed
date: YYYY-MM-DD
authors:
  - Name
supersedes: ~
superseded-by: ~
---

# ADR-NNNN: Title

## Status

Proposed

## Context

What situation, problem, or constraint requires a decision?

## Decision

What did we decide to do?

## Consequences

### Positive
-

### Negative
-

### Risks
-

## Alternatives Considered

### Option A
Why not chosen.

## References
-
```

ADR rules: never edit once accepted — only supersede. Number sequentially, zero-padded to 4 digits (`0001-title.md`).

---

## plans/

### index.md

Must contain: link to roadmap.md, table of active epics with status, table of active feature specs with status and epic link.

### roadmap.md

Must contain: timeline horizon, ordered priority list, what is explicitly not planned and why, who owns this roadmap.

### epics/index.md

Index table of all epics with status.

### features/index.md

Index table of all feature specs with status and epic link.

---

## api/

### index.md

List all files with one-line descriptions.

### endpoints.md

Table of routes: method, path, auth required, description.

### authentication.md

Must contain: authentication mechanism, token lifecycle (issuance, refresh, revocation).

### errors.md

Must contain: error response format (JSON example), error code table with HTTP status, description, and whether retryable.

### rate-limiting.md

Rate limits, quotas, and how to handle 429 responses.

### changelog.md

API version history, newest first.

---

## configuration/

### environment.md

Table of every environment variable: name, type, default, required, description. No actual secret values — reference storage location instead.

### feature-flags.md

Table of flags: name, default state, description, expected behaviour.

### secrets.md

Table of secrets: name, storage location, rotation policy. No actual values.

---

## integrations/

### index.md

Table of all integrations with a link to each file.

### <service-name>.md

Must contain: what we use this service for, authentication method and where credentials are stored, key endpoints/operations used, known failure modes and handling, rate limits or quotas, link to runbook if one exists.

---

## security/

### threat-model.md

Must contain: asset table (asset, sensitivity), threat/mitigation table (threat, likelihood, impact, mitigation).

### auth.md

Must contain: authn mechanism, authz model (RBAC/ABAC/etc), key flows.

### data-handling.md

Must contain: PII classification table (data type, classification, retention), encryption at rest and in transit.

### vulnerability-management.md

Must contain: how to report a vulnerability, SLA and severity classifications.

### incident-response.md

Must contain: severity levels with response times, numbered response steps, escalation contacts.

---

## guides/

### getting-started.md

Must contain: prerequisites, numbered setup steps, verification step.

### development.md

Must contain: how to run locally, key commands table.

### deployment.md

Must contain: per-environment deployment instructions, rollback procedure.

### testing.md

Must contain: how to run tests, table of test suites with command and coverage scope.

### contributing.md

Must contain: PR process, review expectations, code standards.

---

## operations/

### monitoring.md

Must contain: dashboards table (name, URL, what it shows), alerts table (alert, condition, severity, runbook link).

### disaster-recovery.md

Must contain: RTO/RPO targets table, recovery procedures.

### capacity-planning.md

Must contain: current scale, growth triggers table (metric, threshold, action).

### runbooks/index.md

Table of all runbooks: name, trigger condition, estimated duration, last tested date.

### runbooks/<name>.md

Every runbook must contain: **Trigger**, **Prerequisites**, numbered **Steps**, **Verification**, **Rollback**, **Contacts**.

---

## migrations/

### index.md

Table of all migrations: date, description, destructive (yes/no), file link.

### YYYYMMDD-description.md

Must contain: date and description, schema/data changes made, how to run, how to reverse, whether destructive or requires downtime.

---

## changelog/

### index.md

Table of releases: date, version, file link.

### YYYY-MM-DD-vX.Y.Z.md

Release notes: what changed, links to relevant ADRs or feature specs.
