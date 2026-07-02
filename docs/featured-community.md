# Featured Community Items

The Community > Featured page showcases up to 6 curated items from the AI coding community. Items can be skills, MCP servers, collections, templates, commands, instructions, rules, sub-agents, hooks, plugins, or external third-party tools.

## Source file

`src-tauri/assets/marketplace/featured-community.json`

The file is a JSON array of item objects. It is embedded in the binary at compile time.

## Adding or replacing an item

Append a new object to the array, or replace an existing placeholder entry. Set `"placeholder": true` for empty "Coming Soon" slots. Replace a placeholder by filling in the fields and setting `"placeholder": false` (or removing the field).

The grid displays up to 6 items. Placeholder items render as non-interactive dashed-border cards with a clock icon.

## Full schema

```json
{
  "id": "unique-kebab-case-id",
  "name": "Display Name",
  "type": "plugin",
  "description": "One to two sentence short description shown on the card.",
  "about": "Extended markdown description shown on the detail page.\n\nSupports paragraphs, bullet lists, and inline [links](https://example.com).",
  "icon": "example.com",
  "author": {
    "name": "Product or Organisation Name",
    "url": "https://example.com/"
  },
  "creator": {
    "name": "Person Name",
    "bio": "Optional one-line bio.",
    "picture": "https://github.com/username.png",
    "url": "https://github.com/username"
  },
  "links": {
    "website": "https://example.com/",
    "github": "https://github.com/org/repo"
  },
  "marketplace_target": {
    "tab": "skill-store",
    "id": "owner/repo/skill-name"
  },
  "app_target": {
    "tab": "tools",
    "label": "View Tool"
  },
  "plugin_target": {
    "plugin_id": "plugin-name"
  },
  "external_url": "https://example.com/",
  "tags": ["tag1", "tag2", "tag3"],
  "placeholder": false
}
```

## Field reference

### `id`

Unique kebab-case identifier. For real items use the product name (e.g. `"example-tool"`). For placeholders use `"placeholder-N"`.

### `type`

Determines the badge shown on the card. One of:

| Value | Label |
|---|---|
| `"skill"` | Skill |
| `"collection"` | Collection |
| `"template"` | Template |
| `"mcp-server"` | MCP Server |
| `"command"` | Command |
| `"instruction"` | Instruction |
| `"rule"` | Rule |
| `"sub-agent"` | Sub-Agent |
| `"hook"` | Hook |
| `"plugin"` | Plugin |
| `"external"` | External |

### `description`

Short text shown on the card grid. One to two sentences. No markdown.

### `about`

Extended description shown on the detail page. Supports:

- Paragraphs separated by `\n\n`
- Bullet lists with `\n- ` prefix
- Inline links with `[text](url)` syntax

Set to `""` for placeholders.

### `icon`

A Brandfetch-resolvable domain (e.g. `"example.com"`). Set to `null` to fall back to a letter avatar derived from the name.

### `author`

The product or organisation. Both fields are required strings.

| Field | Description |
|---|---|
| `name` | Display name of the product or org |
| `url` | Homepage URL |

For placeholders use `{ "name": "", "url": "" }`.

### `creator`

Optional. The individual person behind the item. Set to `null` when there is no individual creator to credit.

| Field | Required | Description |
|---|---|---|
| `name` | Yes | Person's display name |
| `bio` | No | One-line bio. `null` to omit |
| `picture` | No | Avatar URL, typically `https://github.com/{user}.png`. `null` to omit |
| `url` | No | Profile link. `null` to omit |

### `links`

External links shown on the detail page. Both `website` and `github` accept a URL string or `null`.

### `marketplace_target`

For items that exist in the Marketplace tabs. Set to `null` when the item is not in the marketplace.

| Field | Description |
|---|---|
| `tab` | One of: `"skill-store"`, `"mcp-marketplace"`, `"template-marketplace"`, `"collection-marketplace"` |
| `id` | The item's identifier within that marketplace |

### `app_target`

For items that link to an arbitrary in-app tab. Set to `null` when not applicable. Do not use for plugins — use `plugin_target` instead.

| Field | Description |
|---|---|
| `tab` | Any valid tab ID (e.g. `"tools"`, `"skills"`, `"mcp"`) |
| `label` | Button label (e.g. `"View Tool"`) |

### `plugin_target`

For items that are Automatic plugins. The UI checks whether the plugin is enabled and renders the appropriate button:

- **Enabled** — "View Plugin" — navigates to Library > Tools
- **Not enabled** — "Enable Plugin" — navigates to Settings > Plugins

`plugin_id` must match the plugin's `id` as registered in `src/plugins/index.ts`. Set to `null` for non-plugin items.

### `external_url`

URL opened in the system browser when the user clicks "Visit Website". This is the primary action for items without a `marketplace_target`. Set to `null` for items that only navigate within the app.

### `tags`

Lowercase, hyphenated strings. Shown as pills on the card. Keep to 3-5 tags.

### `placeholder`

Set to `true` for "Coming Soon" slots. These render as non-interactive dashed-border cards. Omit or set to `false` for real items.

## Navigation priority

The detail page renders action buttons in this order:

1. `plugin_target` — plugin-aware button (View Plugin / Enable Plugin)
2. `app_target` — in-app navigation button
3. `external_url` — "Visit Website" button (opens system browser)

Multiple can be present simultaneously (e.g. a plugin with both "View Plugin" and "Visit Website" buttons).

## Placeholder entry

```json
{
  "id": "placeholder-1",
  "name": "Coming Soon",
  "type": "skill",
  "description": "A new community skill will be featured here.",
  "about": "",
  "icon": null,
  "author": { "name": "", "url": "" },
  "creator": null,
  "links": { "website": null, "github": null },
  "marketplace_target": null,
  "app_target": null,
  "plugin_target": null,
  "external_url": null,
  "tags": [],
  "placeholder": true
}
```

## Real entry example

```json
{
  "id": "example-tool",
  "name": "Example Tool",
  "type": "plugin",
  "description": "One to two sentences describing what the tool does.",
  "about": "Extended description of the tool.\n\nIntegrates with Automatic as a plugin.\n\n- [Website](https://example.com/)\n- [Documentation](https://example.com/docs)",
  "icon": "example.com",
  "author": {
    "name": "Example Tool",
    "url": "https://example.com/"
  },
  "creator": {
    "name": "Jane Doe",
    "bio": null,
    "picture": "https://github.com/example-user.png",
    "url": "https://github.com/example-user"
  },
  "links": {
    "website": "https://example.com/",
    "github": null
  },
  "marketplace_target": null,
  "app_target": null,
  "plugin_target": {
    "plugin_id": "example-tool"
  },
  "external_url": "https://example.com/",
  "tags": ["tag1", "tag2", "tag3"],
  "placeholder": false
}
```

## Related files

| File | Purpose |
|---|---|
| `src-tauri/assets/marketplace/featured-community.json` | Data source |
| `src/pages/community/Featured.tsx` | Page component |
| `src/App.tsx` | Tab/section registration (community section) |
