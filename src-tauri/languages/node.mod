id = "node"
name = "Node.js"

# Matches any package.json that isn't claimed by a more specific framework module.
# In practice the engine evaluates all modules independently; the snapshot builder
# deduplicates entry_points across matched modules so there is no double-inclusion.
[[detect]]
files = ["package.json"]

config_files = ["package.json", "tsconfig.json", ".nvmrc", ".node-version"]

ignore_dirs = ["node_modules", "dist", "build", ".turbo"]

entry_points = [
    "index.ts",
    "index.js",
    "index.mjs",
    "index.cjs",
    "src/index.ts",
    "src/index.js",
    "src/main.ts",
    "src/main.js",
    "bin/index.js",
    "bin/cli.js",
    "cli.ts",
    "cli.js",
    "server.ts",
    "server.js",
    "app.ts",
    "app.js",
]
