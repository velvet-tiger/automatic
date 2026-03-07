id = "php"
name = "PHP"

[[detect]]
files = ["composer.json"]

config_files = ["composer.json", "composer.lock"]

ignore_dirs = ["vendor", "node_modules", "storage", "bootstrap/cache"]

entry_points = [
    "public/index.php",
    "index.php",
    "artisan",
    "routes/web.php",
    "routes/api.php",
    "src/index.php",
]
