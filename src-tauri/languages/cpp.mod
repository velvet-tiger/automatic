id = "cpp"
name = "C / C++"

[[detect]]
files = ["CMakeLists.txt"]

[[detect]]
files = ["Makefile"]

config_files = ["CMakeLists.txt", "Makefile", "conanfile.txt", "vcpkg.json"]

ignore_dirs = ["build", "cmake-build-debug", "cmake-build-release", ".cache"]

entry_points = [
    "main.cpp",
    "main.c",
    "src/main.cpp",
    "src/main.c",
    "include/main.h",
]
