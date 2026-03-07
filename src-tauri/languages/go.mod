id = "go"
name = "Go"

[[detect]]
files = ["go.mod"]

config_files = ["go.mod", "go.sum"]

ignore_dirs = ["vendor"]

entry_points = [
    "main.go",
    "cmd/main.go",
    "cmd/root.go",
    "internal/app.go",
    "internal/server.go",
]
