id = "svelte"
name = "Svelte / SvelteKit"

[[detect]]
files = ["package.json"]
contains = ["\"svelte\""]

config_files = ["package.json", "svelte.config.js", "svelte.config.ts", "vite.config.ts", "tsconfig.json"]

ignore_dirs = ["node_modules", ".svelte-kit", "build", "dist"]

entry_points = [
    "src/routes/+page.svelte",
    "src/routes/+layout.svelte",
    "src/app.html",
    "src/main.ts",
    "src/main.js",
    "src/App.svelte",
]
