---
name: automatic-docs-find
description: Find the right documentation file in a project using the standard Automatic docs/ structure. Use when asked where to find something, where to document something, which file covers a topic, or when about to read or write docs and need to know the correct location. Maps questions and topics to canonical file paths.
---

# Automatic Docs Finder

Map any question or topic to the correct file in the standard `docs/` structure.

## Lookup Table

| If the question is about… | Go to |
|---------------------------|-------|
| What this project is, who it's for, why it exists | `docs/context/product.md` |
| Domain terms, vocabulary, ubiquitous language | `docs/context/domain.md` |
| Who the stakeholders are, who decides what | `docs/context/stakeholders.md` |
| How the system works at a high level | `docs/architecture/overview.md` |
| Data model, entities, relationships, storage | `docs/architecture/data-model.md` |
| API style, versioning strategy, auth patterns | `docs/architecture/api-design.md` |
| Cloud resources, infrastructure, deployment topology | `docs/architecture/infrastructure.md` |
| What services exist and what they do | `docs/architecture/services.md` |
| Technical debt, scale limits, hard constraints | `docs/architecture/constraints.md` |
| Why a technical decision was made | `docs/adr/` — search by topic |
| What decisions have been made (index) | `docs/adr/index.md` |
| What's planned, roadmap, upcoming work | `docs/plans/roadmap.md` |
| A specific feature spec | `docs/plans/features/` |
| A specific epic | `docs/plans/epics/` |
| API endpoints, routes, request/response shapes | `docs/api/endpoints.md` |
| How to authenticate with the API | `docs/api/authentication.md` |
| API error codes and retry behaviour | `docs/api/errors.md` |
| API rate limits | `docs/api/rate-limiting.md` |
| API version history | `docs/api/changelog.md` |
| Environment variables | `docs/configuration/environment.md` |
| Feature flags | `docs/configuration/feature-flags.md` |
| Secrets, where they're stored, rotation | `docs/configuration/secrets.md` |
| A third-party service integration | `docs/integrations/<service-name>.md` |
| Threat model, security risks | `docs/security/threat-model.md` |
| Auth design, login flows, permissions | `docs/security/auth.md` |
| PII, data retention, encryption | `docs/security/data-handling.md` |
| How vulnerabilities are handled | `docs/security/vulnerability-management.md` |
| What to do during a security incident | `docs/security/incident-response.md` |
| First-time setup, getting the project running | `docs/guides/getting-started.md` |
| Day-to-day development workflow | `docs/guides/development.md` |
| How to deploy | `docs/guides/deployment.md` |
| How to run tests | `docs/guides/testing.md` |
| PR process, code standards, how to contribute | `docs/guides/contributing.md` |
| Dashboards, metrics, alerts | `docs/operations/monitoring.md` |
| Disaster recovery, RTO/RPO | `docs/operations/disaster-recovery.md` |
| Capacity planning, scaling triggers | `docs/operations/capacity-planning.md` |
| Step-by-step operational procedure (runbook) | `docs/operations/runbooks/<name>.md` |
| Database or data migrations | `docs/migrations/` |
| What changed in a release | `docs/changelog/` |

## Workflow

1. **Identify the topic** from the user's question or task.
2. **Map it** using the lookup table above. When multiple files could apply, name all of them.
3. **Check whether the file exists** before reading it — the project may not have scaffolded every section.
4. **If the file doesn't exist**, say so and suggest either creating it (load `automatic-docs` skill) or checking `docs/index.md` for what is available.
5. **If the topic doesn't fit any section**, it likely belongs in `docs/context/` (domain knowledge) or a new `docs/guides/` entry.

## Where to Write New Content

Apply the same table in reverse — match the topic to the correct file before writing. If no file fits:

- Operational knowledge → new runbook in `docs/operations/runbooks/`
- A significant technical decision → new ADR in `docs/adr/`
- A new integration → new file in `docs/integrations/`
- A new feature spec → new file in `docs/plans/features/`
- Anything else → ask the user before creating a file outside the standard structure
