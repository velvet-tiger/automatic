---
name: project-template-author
description: Use when creating or editing Automatic project templates in src-tauri/project-templates/. Invoke for new templates, updating skills lists, writing unified instructions, or ensuring bundled skill files are in place.
license: MIT
metadata:
  author: local
  version: "1.0.0"
  domain: automatic
  triggers: project template, new template, add template, create template, edit template, unified_instruction, template skills
  role: specialist
  scope: implementation
  output-format: json
  related-skills: automatic, skill-builder
---

# Project Template Author

Expert in authoring Automatic project templates — the JSON files in `src-tauri/project-templates/` that define reusable project configurations for agents.

## Role Definition

You understand the full structure of an Automatic project template, the relationship between templates and bundled skills, and how the `unified_instruction` field is used by agents when working inside a project of that type.

## When to Use This Skill

- Creating a new project template JSON file
- Adding or removing skills from an existing template
- Writing or improving a `unified_instruction` block
- Ensuring the referenced skills exist in `src-tauri/skills/`
- Auditing templates for consistency and completeness

## Template Schema

Every template lives at `src-tauri/project-templates/<name>.json` and must conform to this structure:

```json
{
  "name": "kebab-case-identifier",
  "display_name": "Human Readable Name",
  "icon": "domain.tld",
  "description": "One to two sentence description of what this template is for and who it targets.",
  "category": "One of: API / Backend | Web Application | Desktop App | Frontend | Infrastructure | Data & Analytics | Mobile | Other",
  "tags": ["lowercase", "kebab-case", "tags"],
  "skills": ["skill-name-1", "skill-name-2"],
  "mcp_servers": [],
  "providers": [],
  "agents": [],
  "project_files": [],
  "unified_instruction": "# ...",
  "unified_rules": []
}
```

### Field Reference

| Field | Type | Notes |
|-------|------|-------|
| `name` | string | Kebab-case, matches filename without `.json`. Must be unique. |
| `display_name` | string | Shown in the UI. Title case. |
| `icon` | string | A domain name — used to fetch a brand logo via Brandfetch (e.g. `laravel.com`, `python.org`). |
| `description` | string | 1–2 sentences. Mention the stack, key features, and target audience. |
| `category` | string | Pick the closest match from the allowed list. |
| `tags` | string[] | Lowercase, kebab-case. Used for search and filtering. Include language, framework, key libraries. |
| `skills` | string[] | Names of skills to activate when working in a project of this type. Must match installed skill directory names. |
| `mcp_servers` | string[] | MCP server names to attach. Usually empty for templates — set at project level. |
| `providers` | string[] | LLM provider names. Usually empty for templates. |
| `agents` | string[] | Agent tool names (e.g. `claude`, `codex`, `opencode`). Usually empty for templates. |
| `project_files` | string[] | Reserved for future use. Leave empty. |
| `unified_instruction` | string | Markdown string (escaped). The primary agent context document — see below. |
| `unified_rules` | any[] | Reserved for future use. Leave empty array. |

## Writing `unified_instruction`

The `unified_instruction` is the most important field. It is injected as agent context whenever an agent works in a project created from this template. Write it as if briefing a senior engineer joining the project cold.

### Structure

```markdown
# <Template Display Name>

## Stack
- **<Layer>**: <Technology and version>
- ...

## Project Structure
\`\`\`
<annotated directory tree>
\`\`\`

## Conventions
- <Bullet per key convention — be specific, not generic>

## Key Commands
\`\`\`bash
<command>   # What it does
\`\`\`
```

### Rules for `unified_instruction`

- **Be specific to the stack** — never write generic advice that applies to any project. "Write tests" is useless; "Every endpoint has a corresponding Pest feature test" is useful.
- **Version-pin** — always include major versions (e.g. `Laravel 11+`, `Python 3.12+`, `Terraform 1.7+`).
- **Conventions are opinionated** — the template represents a specific way of working. State it clearly.
- **Commands must be runnable** — include the actual CLI invocations an agent would use to build, test, and run the project.
- **Escape for JSON** — the value is a JSON string. Newlines must be `\n`, backtick blocks must use `\`\`\`` (no escaping needed in JSON strings, but be aware of nesting).

## Skills

### Choosing Skills

Skills listed in `"skills"` are loaded as context when an agent works in a project of this type. Choose skills that:

1. Match the **primary language** of the stack (e.g. `python-pro`, `php-pro`)
2. Match the **primary framework** (e.g. `laravel-specialist`, `vercel-react-best-practices`)
3. Match **supporting tools** that have their own skill (e.g. `pennant-development`, `terraform-skill`)

Do not add skills that are generic or weakly related. Prefer quality over quantity — 2–3 well-chosen skills beats 6 loosely related ones.

### Bundling Skills

Every skill referenced in a template's `"skills"` array **must** have a corresponding directory in `src-tauri/skills/<skill-name>/`. This is the bundled copy shipped with the application.

Steps to add a skill:
1. Confirm the skill is installed globally at `~/.agents/skills/<name>/`
2. Copy the entire skill directory: `cp -r ~/.agents/skills/<name> src-tauri/skills/<name>`
3. Add the skill name to the template's `"skills"` array

If the skill does not exist globally, install it first:
```bash
npx skills add <owner/repo@skill-name> -g -y
```

## Constraints

### MUST DO
- Validate that every skill in `"skills"` has a matching directory in `src-tauri/skills/`
- Use kebab-case for `name` and all tags
- Set `icon` to a real domain that has a recognisable brand logo
- Write `unified_instruction` in full — never use placeholder content
- Escape the `unified_instruction` value correctly as a JSON string

### MUST NOT DO
- Add skills that are not installed and bundled
- Leave `unified_instruction` empty or generic
- Invent skill names — only use skills that actually exist
- Use `mcp_servers`, `providers`, or `agents` in templates (those are set per-project)
- Create duplicate templates for the same stack

## Existing Templates

| File | Stack | Skills |
|------|-------|--------|
| `laravel-api-backend.json` | Laravel 11, PHP 8.3, Pest, Horizon, Pennant | `laravel-specialist`, `pennant-development`, `php-pro` |
| `nextjs-saas-starter.json` | Next.js 14 App Router, Tailwind, Prisma, NextAuth | `vercel-react-best-practices`, `tailwindcss-development` |
| `python-data-pipeline.json` | Python 3.12, Pydantic, DuckDB, asyncio, pytest | `python-pro` |
| `react-component-library.json` | React 18, TypeScript, Storybook, Vitest, Rollup | `vercel-react-best-practices`, `tailwindcss-development` |
| `tauri-desktop-app.json` | Tauri 2, Rust, React, TypeScript, Tailwind | `tailwindcss-development`, `vercel-react-best-practices` |
| `terraform-aws-infrastructure.json` | Terraform 1.7, AWS, GitHub Actions, Terratest | `terraform-skill`, `github-workflow-automation` |

## Core Workflow

1. **Identify the stack** — confirm language, framework, key libraries, and versions
2. **Find skills** — check `src-tauri/skills/` for existing matches; search `skills.sh` for gaps; install and bundle any new ones
3. **Write the template JSON** — fill every field; write `unified_instruction` in full
4. **Verify** — confirm all referenced skills exist in `src-tauri/skills/`; validate JSON is well-formed
