# Automatic Codex Plugin

This repo-local plugin exposes Automatic through Codex's plugin system.

## What it includes

- a plugin manifest at `.codex-plugin/plugin.json`
- an MCP server definition at `.mcp.json`
- a bundled `automatic` skill under `skills/automatic/`
- a repo marketplace entry at `.agents/plugins/marketplace.json`

## MCP server

The plugin registers the Automatic desktop binary as:

```json
{
  "mcpServers": {
    "automatic": {
      "command": "automatic",
      "args": ["mcp-serve"]
    }
  }
}
```

That assumes the `automatic` binary is available on `PATH`.

## Important note

Automatic can already sync the same MCP server into project-level agent configs.

If a workspace already has an `automatic` MCP entry managed by Automatic in `.codex/config.toml`, installing this plugin may create a second registration path for the same server. In that case, prefer one source of truth:

- use the plugin for global Codex access, or
- use Automatic's per-project sync

Avoid enabling both unless you specifically want both layers and have verified Codex's merge behavior.
