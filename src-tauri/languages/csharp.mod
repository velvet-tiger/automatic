id = "csharp"
name = "C# / .NET"

# Detection relies on file extensions (.csproj / .sln) which can't be expressed
# as a simple filename list. The engine handles the glob_extensions field for
# this case: match if any root-level file has one of these extensions.
glob_extensions = [".csproj", ".sln"]

config_files = []

ignore_dirs = ["bin", "obj", ".vs", "packages"]

entry_points = [
    "Program.cs",
    "Startup.cs",
    "src/Program.cs",
    "src/Startup.cs",
]
