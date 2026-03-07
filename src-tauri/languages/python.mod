id = "python"
name = "Python"

[[detect]]
files = ["pyproject.toml"]

[[detect]]
files = ["setup.py"]

[[detect]]
files = ["requirements.txt"]

[[detect]]
files = ["Pipfile"]

config_files = ["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "Pipfile"]

ignore_dirs = ["__pycache__", ".venv", "venv", "env", ".tox", ".eggs", ".pytest_cache", ".mypy_cache", ".ruff_cache", "dist", "build"]

entry_points = [
    "main.py",
    "app.py",
    "run.py",
    "manage.py",
    "wsgi.py",
    "asgi.py",
    "src/main.py",
    "src/app.py",
    "__init__.py",
    "cli.py",
    "server.py",
]
