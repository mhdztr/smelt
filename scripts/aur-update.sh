#!/usr/bin/env bash
# Bump pkgver, refresh sha256sums, and regenerate .SRCINFO for an AUR
# package in packaging/aur/. See docs/AUR.md for the full release runbook.
set -euo pipefail

usage() {
  echo "usage: $0 <smelt|smelt-bin|smelt-git> [version]" >&2
  exit 2
}

[[ $# -ge 1 ]] || usage
pkg="$1"
version="${2:-}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dir="$root/packaging/aur/$pkg"
if [[ ! -f "$dir/PKGBUILD" ]]; then
  echo "error: no PKGBUILD at $dir/PKGBUILD" >&2
  exit 1
fi
cd "$dir"

tarball_url=""
case "$pkg" in
  smelt)
    [[ -n "$version" ]] || usage
    sed -i "s/^pkgver=.*/pkgver=$version/" PKGBUILD
    tarball_url="https://github.com/mhdztr/smelt/archive/refs/tags/v$version.tar.gz"
    ;;
  smelt-bin)
    [[ -n "$version" ]] || usage
    sed -i "s/^pkgver=.*/pkgver=$version/" PKGBUILD
    tarball_url="https://github.com/mhdztr/smelt/releases/download/v$version/smelt-x86_64-unknown-linux-gnu.tar.gz"
    ;;
  smelt-git)
    echo "note: smelt-git tracks git HEAD; pkgver is computed by pkgver(), not bumped here."
    ;;
  *)
    echo "error: unknown package '$pkg'" >&2
    exit 1
    ;;
esac

if [[ -n "$tarball_url" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  echo "downloading $tarball_url"
  # crates.io/GitHub reject the default curl UA sometimes; set one explicitly.
  if curl -fsSL -A "smelt-aur-update/1.0" -o "$tmp/$pkg-$version.tar.gz" "$tarball_url"; then
    sum="$(sha256sum "$tmp/$pkg-$version.tar.gz" | cut -d' ' -f1)"
    sed -i "s/^sha256sums=.*/sha256sums=('$sum')/" PKGBUILD
    echo "sha256sums -> $sum"
  else
    echo "warning: could not download the tarball (tag or release asset not published yet);" >&2
    echo "warning: leaving sha256sums untouched -- do NOT publish with placeholder sums." >&2
    # Never continue to the .SRCINFO step after a failed download: the script
    # would bump pkgver, keep the old checksum, and exit 0, which invites a
    # publish with a stale sha256.
    exit 1
  fi
fi

# Generate into a temp file and only replace the tracked .SRCINFO after
# makepkg succeeds: under `set -e` a redirect into .SRCINFO truncates the
# committed file before the failure is visible.
srcinfo_tmp="$(mktemp .SRCINFO.XXXXXX)"
if ! makepkg --printsrcinfo > "$srcinfo_tmp"; then
  rm -f "$srcinfo_tmp"
  echo "error: makepkg --printsrcinfo failed; .SRCINFO left unchanged" >&2
  exit 1
fi
mv "$srcinfo_tmp" .SRCINFO
echo "updated $dir (pkgver + .SRCINFO)."
echo "next: review the diff, copy PKGBUILD and .SRCINFO into the AUR clone," >&2
echo "commit (message: 'up ${version:-HEAD}') and push." >&2
