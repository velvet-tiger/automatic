id = "react"
name = "React"

[[detect]]
files = ["package.json"]
contains = ["\"react\""]

config_files = ["package.json", "vite.config.ts", "vite.config.js", "tsconfig.json"]

ignore_dirs = ["node_modules", "dist", ".next", "out"]

entry_points = [
    "src/main.tsx",
    "src/main.ts",
    "src/main.jsx",
    "src/main.js",
    "src/index.tsx",
    "src/index.ts",
    "src/index.jsx",
    "src/index.js",
    "src/App.tsx",
    "src/App.ts",
    "src/App.jsx",
    "src/App.js",
    "index.tsx",
    "index.ts",
    "index.js",
]
