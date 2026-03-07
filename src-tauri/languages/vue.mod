id = "vue"
name = "Vue"

[[detect]]
files = ["package.json"]
contains = ["\"vue\""]

config_files = ["package.json", "vite.config.ts", "vite.config.js", "vue.config.js", "tsconfig.json"]

ignore_dirs = ["node_modules", "dist", ".nuxt", "out"]

entry_points = [
    "src/main.ts",
    "src/main.js",
    "src/App.vue",
    "src/router/index.ts",
    "src/router/index.js",
    "src/store/index.ts",
    "src/store/index.js",
]
