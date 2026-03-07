id = "ruby"
name = "Ruby"

[[detect]]
files = ["Gemfile"]

config_files = ["Gemfile", "Gemfile.lock", "config/application.rb"]

ignore_dirs = ["vendor", "tmp", ".bundle", "log", "public/assets", "public/packs"]

entry_points = [
    "app.rb",
    "config/application.rb",
    "config/routes.rb",
    "lib/main.rb",
    "bin/rails",
    "config/environment.rb",
    "Rakefile",
]
