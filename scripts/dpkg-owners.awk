# Prints the packages that own a file, from `dpkg -S` output.
#
# Usage: awk -v suffix=/libfoo.so.1 -f scripts/dpkg-owners.awk <dpkg -S output>
#
# Reads lines of the form "pkg[:arch][, pkg[:arch]...]: /path" and prints each
# package (without its architecture) whose path ends with `suffix`, one per
# line, possibly repeated. Lines about diversions ("diversion by ...",
# "local diversion from: ...", "local diversion to: ...") name no package and
# are skipped. scripts/appimage-system-licenses.sh uses it, and
# scripts/dpkg-owners.test.ts tests it.

/^(local )?diversion / { next }

{
  split_at = index($0, ": /")
  if (split_at == 0) next
  path = substr($0, split_at + 2)
  if (length(path) < length(suffix)) next
  if (substr(path, length(path) - length(suffix) + 1) != suffix) next
  n = split(substr($0, 1, split_at - 1), pkgs, ", ")
  for (i = 1; i <= n; i++) {
    sub(/:.*/, "", pkgs[i])
    print pkgs[i]
  }
}
