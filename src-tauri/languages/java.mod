id = "java"
name = "Java / Kotlin"

[[detect]]
files = ["pom.xml"]

[[detect]]
files = ["build.gradle"]

[[detect]]
files = ["build.gradle.kts"]

config_files = ["pom.xml", "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"]

ignore_dirs = ["target", "build", ".gradle", "out", ".idea"]

# Java entry points are too variable (depend on package structure); the directory
# tree gives enough context for the AI to locate them.
entry_points = []
