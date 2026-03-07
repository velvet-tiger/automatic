id = "nextjs"
name = "Next.js"

[[detect]]
files = ["package.json"]
contains = ["\"next\""]

config_files = ["package.json", "next.config.js", "next.config.ts", "next.config.mjs", "tsconfig.json"]

ignore_dirs = ["node_modules", ".next", "out", "dist"]

entry_points = [
    "app/page.tsx",
    "app/page.ts",
    "app/layout.tsx",
    "app/layout.ts",
    "src/app/page.tsx",
    "src/app/layout.tsx",
    "pages/index.tsx",
    "pages/index.ts",
    "pages/index.js",
    "pages/_app.tsx",
    "pages/_app.ts",
    "pages/_app.js",
    "src/pages/index.tsx",
    "src/pages/_app.tsx",
]
