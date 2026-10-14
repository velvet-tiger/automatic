#!/usr/bin/env bash
# Write the licences of the Ubuntu system libraries that the Linux AppImage
# carries.
#
# Usage: scripts/appimage-system-licenses.sh <AppImage> <output-file> <file-list>
#
# Runs on the Ubuntu machine that built the AppImage, because it reads that
# machine's package database. It unpacks the AppImage, finds the Debian
# package that owns each file linuxdeploy copied into it, and writes
# <output-file> with each package's version, source package and copyright
# file, followed by the full text of every licence those copyright files
# refer to under /usr/share/common-licenses. <file-list> gets the sorted list
# of files in the AppImage, so that a second build can be checked against it.
#
# It fails when a file maps to no package or to more than one, when a package
# has no copyright file, when a copyright file refers to a licence text the
# machine does not have, and when a package on the deny list below is
# included.
set -euo pipefail

appimage=${1:?usage: appimage-system-licenses.sh <AppImage> <output-file> <file-list>}
out=${2:?usage: appimage-system-licenses.sh <AppImage> <output-file> <file-list>}
list=${3:?usage: appimage-system-licenses.sh <AppImage> <output-file> <file-list>}

# Packages whose licence the app's licence cannot sit beside. libjbig0
# (JBIG-KIT) is GPL-2+ only. Only libtiff loads it, and only the gdk-pixbuf
# TIFF loader loads libtiff, so release.yml sets that loader aside before
# bundling. This check stops a release if it comes back.
deny_packages=" libjbig0 "

# Files that are not copies of Ubuntu files. The app's own files, the
# AppImage start-up files (their notices are in THIRD_PARTY_NOTICES.md),
# the copyright files linuxdeploy copies, and caches generated while
# bundling.
is_own_or_generated() {
  case "$1" in
    AppRun | AppRun.wrapped | .DirIcon | Automatic.desktop | Automatic.png) return 0 ;;
    apprun-hooks/*) return 0 ;;
    usr/bin/automatic) return 0 ;;
    usr/lib/Automatic/*) return 0 ;;
    usr/share/applications/* | usr/share/icons/*) return 0 ;;
    usr/share/doc/*) return 0 ;;
    usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache) return 0 ;;
    usr/lib/gtk-3.0/3.0.0/immodules.cache) return 0 ;;
    usr/share/glib-2.0/schemas/gschemas.compiled) return 0 ;;
  esac
  return 1
}

# The part of a file's AppImage path that its system path ends with.
# linuxdeploy flattens libraries into usr/lib, so usr/lib/libfoo.so.1 came
# from some .../libfoo.so.1 on the build machine.
system_suffix() {
  case "$1" in
    usr/lib/*) printf '/%s\n' "${1#usr/lib/}" ;;
    usr/*) printf '/%s\n' "${1#usr/}" ;;
    *) printf '/%s\n' "$1" ;;
  esac
}

script_dir=$(cd "$(dirname "$0")" && pwd)
appimage_abs=$(realpath "$appimage")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
(cd "$work" && "$appimage_abs" --appimage-extract >/dev/null)
appdir="$work/squashfs-root"
if [ ! -d "$appdir/usr" ]; then
  echo "error: $appimage did not unpack to a squashfs-root with a usr folder" >&2
  exit 1
fi

(cd "$appdir" && find . \( -type f -o -type l \) -printf '%P\n' | LC_ALL=C sort) >"$work/all-files"
grep -v '^usr/lib/Automatic/' "$work/all-files" >"$list" || true

# Symlinks the bundling tools made point at files that are mapped on their
# own. A symlink that leads outside the AppImage would be a broken copy.
: >"$work/system-files"
while IFS= read -r file; do
  is_own_or_generated "$file" && continue
  if [ -L "$appdir/$file" ]; then
    target=$(realpath -m "$appdir/$file")
    case "$target" in
      "$appdir"/*) [ -e "$target" ] && continue ;;
    esac
    echo "error: $file is a symlink to '$target', which is not a file inside the AppImage" >&2
    exit 1
  fi
  echo "$file" >>"$work/system-files"
done <"$work/all-files"

if [ ! -s "$work/system-files" ]; then
  echo "error: no system files found in $appimage" >&2
  exit 1
fi

# One dpkg call for every file. dpkg prints "pkg[:arch][, pkg...]: /path" for
# each match, and exits non-zero when a pattern matches nothing. Unmatched
# files are caught below. scripts/dpkg-owners.awk reads the output and skips
# the lines dpkg prints about diverted files, such as the TIFF loader
# release.yml sets aside.
patterns=()
while IFS= read -r file; do
  patterns+=("*$(system_suffix "$file")")
done <"$work/system-files"
dpkg -S "${patterns[@]}" >"$work/dpkg-owners" 2>/dev/null || true

: >"$work/file-packages"
failed=0
while IFS= read -r file; do
  suffix=$(system_suffix "$file")
  owners=$(awk -v suffix="$suffix" -f "$script_dir/dpkg-owners.awk" "$work/dpkg-owners" | LC_ALL=C sort -u)
  count=$(printf '%s' "$owners" | grep -c . || true)
  if [ "$count" -eq 0 ]; then
    echo "error: no installed package owns a file ending in '$suffix' (AppImage file $file)" >&2
    failed=1
  elif [ "$count" -gt 1 ]; then
    echo "error: several packages own a file ending in '$suffix' (AppImage file $file): $(printf '%s' "$owners" | tr '\n' ' ')" >&2
    failed=1
  else
    printf '%s\t%s\n' "$owners" "$file" >>"$work/file-packages"
  fi
done <"$work/system-files"
[ "$failed" -eq 0 ] || exit 1

# The match above is by path suffix. linuxdeploy finds libraries through the
# dynamic linker, which looks in /usr/local first. A library of the same name
# there may be the one that was copied, and it belongs to no package.
while IFS=$'\t' read -r _ file; do
  case "$file" in
    usr/lib/*/*) continue ;;
    usr/lib/*) name=${file#usr/lib/} ;;
    *) continue ;;
  esac
  for dir in /usr/local/lib /usr/local/lib64 /usr/local/lib/x86_64-linux-gnu; do
    if [ -e "$dir/$name" ]; then
      echo "error: $dir/$name exists, so the AppImage's $file may not come from the package this script found" >&2
      failed=1
    fi
  done
done <"$work/file-packages"
[ "$failed" -eq 0 ] || exit 1

cut -f1 "$work/file-packages" | LC_ALL=C sort -u >"$work/packages"

while IFS= read -r pkg; do
  case "$deny_packages" in
    *" $pkg "*)
      echo "error: the AppImage carries files from $pkg, which is on the deny list in this script:" >&2
      awk -F'\t' -v p="$pkg" '$1 == p { print "  " $2 }' "$work/file-packages" >&2
      exit 1
      ;;
  esac
  if [ ! -f "/usr/share/doc/$pkg/copyright" ]; then
    echo "error: $pkg has no /usr/share/doc/$pkg/copyright" >&2
    exit 1
  fi
done <"$work/packages"

# The Launchpad address and "apt-get source" line below are right only for
# packages from the Ubuntu archive. Each installed version must be available
# from an Ubuntu archive source, not only from a PPA or a local .deb.
# GitHub's runners reach the archive through a mirror list file.
is_ubuntu_source() {
  case "$1" in
    *ubuntu.com/ubuntu*) return 0 ;;
    mirror+file:*)
      local list=${1#mirror+file:}
      [ -f "$list" ] && ! grep -v -E '^\s*(#|$)' "$list" | grep -v -q 'ubuntu\.com/ubuntu'
      return
      ;;
  esac
  return 1
}
while IFS= read -r pkg; do
  version=$(dpkg-query -W -f='${Version}' "$pkg")
  sources=$(apt-cache policy "$pkg" | awk -v v="$version" '
    /^ (\*\*\*|   ) [^ ]/ { current = ($2 == v); next }
    current && /^        +[0-9]+ / { print $2 }
  ')
  from_ubuntu=0
  for source in $sources; do
    if is_ubuntu_source "$source"; then from_ubuntu=1; fi
  done
  if [ "$from_ubuntu" -eq 0 ]; then
    echo "error: $pkg $version is not available from the Ubuntu archive (sources: $(echo "$sources" | tr '\n' ' '))" >&2
    exit 1
  fi
done <"$work/packages"

# Licence texts the copyright files point to. LGPL-2.1 is always included:
# THIRD_PARTY_NOTICES.md points here for the libfuse copy in the AppImage
# runtime.
{
  echo /usr/share/common-licenses/LGPL-2.1
  while IFS= read -r pkg; do
    grep -oh '/usr/share/common-licenses/[A-Za-z0-9._+-]*' "/usr/share/doc/$pkg/copyright" | sed 's/[.]$//' || true
  done <"$work/packages"
} | LC_ALL=C sort -u >"$work/referenced-licences"

# Some copyright files name a text by its SPDX-style version, such as
# GPL-2.0, where Debian's file is GPL-2. Such a name is printed as the file
# Debian has. A name with neither form fails the run.
: >"$work/common-licenses"
while IFS= read -r licence; do
  if [ -f "$licence" ]; then
    echo "$licence" >>"$work/common-licenses"
  elif [ -f "${licence%.0}" ]; then
    echo "${licence%.0}" >>"$work/common-licenses"
  else
    echo "error: a copyright file refers to $licence, which this machine does not have:" >&2
    while IFS= read -r pkg; do
      grep -l -F "$licence" "/usr/share/doc/$pkg/copyright" >&2 || true
    done <"$work/packages"
    exit 1
  fi
done <"$work/referenced-licences"
LC_ALL=C sort -u -o "$work/common-licenses" "$work/common-licenses"

separator=$(printf '=%.0s' $(seq 1 80))
{
  cat <<'EOF'
Licences of the system libraries in the Automatic AppImage
==========================================================

The Linux AppImage carries copies of libraries and data files from Ubuntu
22.04 packages, so that it runs on systems that lack them. This file lists
each of those packages, the files the AppImage takes from it, and the
package's copyright file. scripts/appimage-system-licenses.sh wrote it from
the AppImage's contents and the build machine's package database.

The files are as Ubuntu ships them, except that linuxdeploy rewrites the
library search path (RPATH) of the shared libraries.

Many of these libraries are licensed under the GNU LGPL. You can replace any
of them. Run the AppImage with --appimage-extract, replace the library in
squashfs-root/usr/lib, and start squashfs-root/AppRun.

Each package's source is available from Ubuntu. Run
"apt-get source <source package>=<version>" on Ubuntu, or download it from
the launchpad.net address given with each package.

The full texts of the licences that the copyright files refer to under
/usr/share/common-licenses are at the end of this file. A reference such as
GPL-2.0 is printed under Debian's name for the same text, GPL-2.
EOF
  while IFS= read -r pkg; do
    info=$(dpkg-query -W -f='${Version}\t${source:Package}\t${source:Version}' "$pkg")
    IFS=$'\t' read -r version source source_version <<<"$info"
    echo
    echo "$separator"
    echo "$pkg $version"
    echo "Source package: $source $source_version"
    echo "https://launchpad.net/ubuntu/+source/$source/$source_version"
    echo
    echo "Files:"
    awk -F'\t' -v p="$pkg" '$1 == p { print "  " $2 }' "$work/file-packages"
    echo
    cat "/usr/share/doc/$pkg/copyright"
  done <"$work/packages"
  while IFS= read -r licence; do
    echo
    echo "$separator"
    echo "$licence"
    echo
    cat "$licence"
  done <"$work/common-licenses"
} >"$out"

# The app shows this file as UTF-8 text and refuses a file that is not.
if ! iconv -f UTF-8 -t UTF-8 "$out" >/dev/null 2>&1; then
  echo "error: $out is not valid UTF-8. A copyright file uses another encoding." >&2
  exit 1
fi

echo "wrote $out: $(wc -l <"$work/packages") packages, $(wc -l <"$work/system-files") files, $(wc -l <"$work/common-licenses") licence texts" >&2
