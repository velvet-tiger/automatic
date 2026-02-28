# Automatic

Automatic is a desktop hub for managing your AI agent configurations. It gives you a central place to organise the skills, MCP servers, rules, and project settings that power tools like Claude Code, Cursor, and other MCP-compatible agents — so your agents always have the right context, wherever they run.

## What it does

Automatic acts as a registry that your agents connect to at the start of each session to discover skills and load project configurations. You manage everything in one place; your agents stay in sync automatically.

## Features

### Dashboard

An at-a-glance overview of your workspace: how many projects you have, which agents are active, how many skills are in use, and how many MCP servers are connected. The dashboard also surfaces a **drift alert** if any project's agent configuration files have fallen out of sync, so you know immediately when something needs attention.

### Projects

Create and manage projects that link a directory on your machine to a set of agent configurations. Each project can have:

- One or more **agents** (Claude Code, Cursor, etc.)
- A set of **skills** to load into those agents
- **MCP servers** to connect
- A description and custom rules

When you sync a project, Automatic writes the appropriate configuration files into your project directory so the agents pick them up automatically. Automatic also monitors for **drift** — if the files on disk diverge from what you've configured here, the dashboard and project list will warn you.

### Agents

Configure the AI agents you work with (Claude Code, Cursor, and others). Agent configurations define how Automatic syncs settings to each tool's expected directory structure and config format.

### Skills

Skills are reusable sets of instructions, workflows, or domain knowledge that you load into an agent for a specific task. Create and edit your own skills in a built-in markdown editor, or install community skills directly from the Skills.sh marketplace. Skills are stored at `~/.agents/skills/` and are available across all your projects.

### Project Templates

Start new projects from pre-built templates that bundle a proven configuration of agents, skills, MCP servers, and rules. Browse the Template Marketplace to find community-contributed templates, or save your own as reusable starting points.

### File Templates

Manage reusable file templates (boilerplate, scaffolds, prompts) that your agents can use when generating code or content.

### Rules

Define standing instructions and constraints that apply across your projects — things like coding conventions, preferred libraries, output format expectations, or tone guidelines. Rules are shared to agent config files alongside skills and MCP server configs.

### MCP Servers

View and manage the MCP (Model Context Protocol) servers registered in your Claude Desktop configuration. MCP servers extend what your agents can do — giving them access to databases, APIs, file systems, and other external tools. Browse the MCP Marketplace to discover and add new servers.

### Settings

Configure global preferences for Automatic, including the skill sync mode (symlink or copy).

### Marketplaces

Three built-in marketplaces let you extend Automatic without leaving the app:

- **Skills Marketplace (Skills.sh)** — Browse and install community skills
- **Templates Marketplace** — Discover project and file templates
- **MCP Servers Marketplace** — Find MCP server integrations to connect to your agents


## Platforms

Automatic runs on macOS, Windows, and Linux.

> **Note:** MCP server config reading currently reads from the Claude Desktop config path on macOS. Cross-platform support for this feature is in progress.

