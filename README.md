# Agentic Desktop

A secure desktop application for managing LLM connections, running local AI agents, and configuring MCP (Model Context Protocol) servers and Skills. Built with Rust, Tauri, React, and Tailwind CSS.

## Features

- **Local Agents**: Run and monitor background AI agent processes securely.
- **LLM Providers**: Securely store API keys in the native OS keychain rather than plain-text.
- **Skills**: Manage and edit `.claude/skills` locally.
- **MCP Servers**: Configure and monitor Model Context Protocol servers.

## Development

1. Install dependencies:
   ```bash
   npm install
   ```
2. Run development server (Frontend + Rust Backend):
   ```bash
   npm run tauri dev
   ```

## Stack

- **Frontend**: React + TypeScript + Tailwind CSS (Vite)
- **Backend**: Rust (Tauri)
- **Key Storage**: `keyring` crate for native OS keychain integration