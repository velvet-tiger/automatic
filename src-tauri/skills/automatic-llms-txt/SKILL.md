---
name: automatic-llms-txt
description: Create and maintain llms.txt files following the llmstxt.org standard. Use when adding LLM-friendly documentation to a project or website.
authors:
  - Automatic
---

# llms.txt

`llms.txt` is a proposed standard for providing LLM-friendly content at the root of a website or project. It gives language models a curated, concise entry point — background context, key links, and pointers to detailed markdown files — without requiring them to parse full HTML pages.

Spec: https://llmstxt.org/

---

## When to create an llms.txt file

Create `llms.txt` when:

- A project has public documentation a developer or agent might need to query at inference time
- You want LLMs to navigate your docs without scraping HTML
- You are publishing a library, API, framework, or product with structured reference material
- The user explicitly asks to "add llms.txt" or "make the project LLM-friendly"

---

## File location

Place `llms.txt` at the root of the site or repository:

```
/llms.txt          ← primary location (web root)
/docs/llms.txt     ← acceptable alternative for doc-only sites
```

---

## Required format

The file uses Markdown with a strict section order:

```markdown
# Project Name

> One-paragraph summary. Key facts an LLM must know to understand the rest of the file.

Optional additional prose (no headings). Use this for important caveats, compatibility notes, or
disambiguation that does not belong in the summary blockquote.

## Section Name

- [Link title](https://url): Brief description of what is at this URL.
- [Another link](https://url)

## Optional

- [Link title](https://url): Content here can be skipped when context is short.
```

### Section rules

| Element | Requirement |
|---|---|
| H1 | Required. Project or site name only. |
| Blockquote (`>`) | Recommended. Short summary, key constraints, important notes. |
| Body prose | Optional. No headings allowed in this block. |
| H2 sections | Optional. Each contains a markdown list of links. |
| `## Optional` | Special: contents may be omitted by tools needing a shorter context window. |

---

## Writing the H1 and summary

**H1** — the name of the project, not a tagline.

**Blockquote** — answer these questions concisely:
- What does this project do?
- What is it *not* (compatibility exclusions, non-goals)?
- What should an LLM know before reading anything else?

```markdown
# Acme SDK

> Acme SDK is a TypeScript client for the Acme REST API (v3+). It does not support v1 or v2
> endpoints. All methods return Promises; there is no callback-style API.
```

---

## Choosing what to link

Each H2 section is a curated list. Link to:

- **Markdown versions of pages** — append `.md` to the URL when the site supports it (e.g. `https://example.com/docs/quickstart.html.md`)
- **Raw GitHub files** — direct `.md` or `.txt` links, not rendered HTML
- **Changelog or migration guides** — especially useful when breaking changes exist
- **OpenAPI / JSON Schema specs** — raw URL, not the Swagger UI page

Avoid linking to:
- Login-required pages
- Pages that return HTML only
- Large auto-generated reference dumps without summaries

### Standard sections (adapt as needed)

```markdown
## Docs
- [Quick start](https://example.com/docs/quickstart.md): Five-minute guide.
- [Configuration reference](https://example.com/docs/config.md): All config keys with defaults.

## API Reference
- [REST API](https://example.com/openapi.json): OpenAPI 3.1 spec.

## Examples
- [Example app](https://github.com/org/repo/blob/main/examples/basic.md): Annotated walkthrough.

## Changelog
- [CHANGELOG](https://github.com/org/repo/blob/main/CHANGELOG.md): Version history.

## Optional
- [Full API reference](https://example.com/docs/full-api.md): Exhaustive method list.
```

---

## Companion files

Consider generating these alongside `llms.txt`:

| File | Purpose |
|---|---|
| `llms-ctx.txt` | Expanded version — inlines all linked content (excluding `## Optional`) |
| `llms-ctx-full.txt` | Expanded version — inlines all linked content including `## Optional` |

Use the [`llms_txt2ctx`](https://llmstxt.org/intro.html#cli) CLI to generate these from `llms.txt`.

---

## Markdown versions of pages

If the project controls its documentation site, expose clean markdown at each page URL with `.md` appended:

```
https://example.com/docs/api.html       ← HTML for humans
https://example.com/docs/api.html.md    ← Markdown for LLMs
```

For URL paths without a file extension, append `index.html.md`:

```
https://example.com/docs/               ← rendered docs
https://example.com/docs/index.html.md  ← markdown version
```

---

## Quality checklist

Before committing `llms.txt`:

- [ ] H1 is the project name, not a tagline or sentence
- [ ] Blockquote covers what the project is and what it is not
- [ ] All linked URLs resolve and return plain text or markdown (not HTML)
- [ ] Descriptions are one concise sentence, not marketing copy
- [ ] `## Optional` section contains only genuinely skippable content
- [ ] No internal or login-required links
- [ ] File is valid Markdown (no broken syntax)
- [ ] File is located at `/llms.txt` in the web root or repository root

---

## Example — complete file

```markdown
# FastHTML

> FastHTML is a python library which brings together Starlette, Uvicorn, HTMX, and fastcore's
> `FT` "FastTags" into a library for creating server-rendered hypermedia applications. It is not
> compatible with FastAPI syntax and is not targeted at creating API services. FastHTML is
> compatible with JS-native web components and vanilla JS libraries, but not React, Vue, or Svelte.

## Docs

- [Quick start](https://fastht.ml/docs/tutorials/quickstart_for_web_devs.html.md): Overview of key FastHTML features.
- [HTMX reference](https://github.com/bigskysoftware/htmx/blob/master/www/content/reference.md): All HTMX attributes, CSS classes, headers, events, extensions, and config options.

## Examples

- [Todo list application](https://github.com/AnswerDotAI/fasthtml/blob/main/examples/adv_app.py): Complete CRUD app showing idiomatic FastHTML and HTMX patterns.

## Optional

- [Starlette documentation](https://gist.githubusercontent.com/jph00/809e4a4808d4510be0e3dc9565e9cbd3/raw/starlette-sml.md): Subset of Starlette docs relevant to FastHTML development.
```
