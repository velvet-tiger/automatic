# Automatic-managed .gitignore

This project ignores the agent configuration that Automatic generates.

Automatic writes the instruction files and agent config directories in this
project. It also keeps a managed block in `.gitignore` that lists those paths.
The block is bounded by these markers:

```
# BEGIN Automatic-managed
...
# END Automatic-managed
```

Follow these rules:

1. Do not commit the ignored files. They are generated. Automatic rewrites them
   on every sync, so committing them causes churn and merge conflicts.
2. Do not edit inside the managed block. Automatic regenerates it on each sync.
   Any manual change between the markers is lost.
3. Do not remove the managed block to force these files into version control. If
   the team wants to share agent config through git, turn off "Manage .gitignore"
   for this project in Automatic instead. That removes the block cleanly.
4. Add your own ignore entries outside the markers. Automatic never touches the
   rest of the file.
