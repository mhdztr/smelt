# docs/AUR.md -- AUR publish runbook

How smelt releases reach the Arch User Repository. Three packages:

| Package | Source | Use case |
| --- | --- | --- |
| `smelt` | tag tarball, built with cargo | most users |
| `smelt-bin` | prebuilt release binary | no Rust toolchain needed |
| `smelt-git` | git HEAD | testers |

Release Linux binaries are dynamically linked against glibc
(`libgcc_s.so.1`, `libm.so.6`, `libc.so.6`, ELF interpreter), so binary
packages depend on `glibc`/`gcc-libs`. A fully static musl artifact is not
part of the 2.0.0 ship claim. 2.0.0 releases publish x86_64 and aarch64 Linux
tarballs plus a wasm web package; `smelt-bin` remains x86_64-only. No
Python/Node runtime is required for native CLI conversions. There are no
remote web services or domain requirements; the browser bindings run client-side
only (served statically on localhost) and not as a backend service.

Note: `gh` is **not** installed on the maintainer machine -- everything here
uses plain `git` plus the browser.

## One-time setup

1. Create an AUR account: https://archlinux.org/account/register/ and
   confirm the email.
2. Add an SSH key on the account page (Account -> SSH Public Keys):
   `cat ~/.ssh/id_ed25519.pub` (create one with `ssh-keygen -t ed25519`
   if absent) and paste it.
3. Test access:
   `ssh -T aur@aur.archlinux.org` -- expect
   `remote: Arch Linux AUR helper for mhdztr` style greeting.
4. Clone the three package repos (they are empty until first push):
   ```sh
   git clone ssh://aur@aur.archlinux.org/smelt.git
   git clone ssh://aur@aur.archlinux.org/smelt-bin.git
   git clone ssh://aur@aur.archlinux.org/smelt-git.git
   ```
   The PKGBUILDs in this repo (`packaging/aur/`) are the source of truth;
   copy them into the corresponding clone on each release.

## Release flow (per version X.Y.Z)

1. Tag in the smelt repo and push (manual; no remote is configured in the
   dev clone yet):
   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
2. Wait for the GitHub tag tarball to exist:
   `https://github.com/mhdztr/smelt/archive/refs/tags/vX.Y.Z.tar.gz` must
   download (GitHub generates it on demand; first fetch can lag a minute).
3. Update the package:
   ```sh
   ./scripts/aur-update.sh smelt X.Y.Z      # bumps pkgver + sha256sums + .SRCINFO
   ./scripts/aur-update.sh smelt-bin X.Y.Z  # same, for the binary package
   # smelt-git needs no version bump; refresh it with:
   ./scripts/aur-update.sh smelt-git        # only regenerates .SRCINFO
   ```
4. Review the diff in each package clone, copy PKGBUILD + .SRCINFO over,
   commit (`up X.Y.Z`), and push:
   ```sh
   cd ~/aur/smelt && git add PKGBUILD .SRCINFO && git commit -m "up X.Y.Z" && git push
   ```
5. Local validation before pushing (never push untested):
   ```sh
   cd packaging/aur/smelt     && makepkg -s          # full build test
   cd packaging/aur/smelt-bin && makepkg -s --nodeps # binary pkg, no deps to fetch
   cd packaging/aur/smelt-git && makepkg -s
   ```
   `makepkg` refuses to run as root -- run as the normal user.

## .SRCINFO regeneration

Always regenerate with makepkg, never by hand:

```sh
cd packaging/aur/<pkg> && makepkg --printsrcinfo > .SRCINFO
```

`scripts/aur-update.sh` does this automatically.

## Common failures

| Symptom | Cause | Fix |
| --- | --- | --- |
| `Integrity checks are not possible` / sha256 mismatch | tarball changed or URL wrong | re-run `aur-update.sh`; verify the tag exists upstream |
| `Failed to retrieve the source tarball` | tag not pushed yet or GitHub lag | push the tag, wait, retry |
| stale `pkgver` shown on the AUR page | pushed PKGBUILD but not .SRCINFO | regenerate and commit .SRCINFO |
| `error: can not build in a root directory` | running makepkg as root | drop to a normal user |
| version went backwards | forgot the epoch after a yanked release | add `epoch=` to the PKGBUILD and bump; never reuse a released pkgver |

## Package review findings

Recorded review of the three PKGBUILDs as committed (a `makepkg` review pass,
not a publish). Verified facts:

| Property | `smelt` | `smelt-bin` | `smelt-git` |
| --- | --- | --- | --- |
| `pkgver` | `2.0.0` | `2.0.0` | `2.0.0.r0.g<commit>` (from `pkgver()`; fallback uses 2.0.0 base, tagged repo strips leading `v`) |
| `pkgrel` | `1` | `1` | `1` |
| `url` | `https://github.com/mhdztr/smelt` | same | same |
| `license` | `MIT` | `MIT` | `MIT` |
| `arch` | `x86_64`, `aarch64` | `x86_64` | `x86_64`, `aarch64` |
| `depends` | `gcc-libs`, `glibc` | `gcc-libs`, `glibc` | `gcc-libs`, `glibc` |
| `makedepends` | `cargo`, `git` | (none) | `git`, `cargo` |
| extra | -- | `provides=('smelt')`, `conflicts=('smelt')`, `options=('!strip')` | `provides=('smelt')`, `conflicts=('smelt')` |
| source | `smelt-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz` | `smelt-bin-$pkgver.tar.gz::$url/releases/download/v$pkgver/smelt-x86_64-unknown-linux-gnu.tar.gz` | `smelt-git::git+https://github.com/mhdztr/smelt.git` |
| checksums | 64-zero placeholder | 64-zero placeholder | `sha256sums=('SKIP')` |

All three share the same `url` and `license=('MIT')`. PKGBUILDs are versioned
for 2.0.0; `smelt` and `smelt-bin` carry placeholder checksums until release
assets are published and `scripts/aur-update.sh` rewrites real sha256 values.
`smelt-bin` remains x86_64-only even though 2.0.0 releases also publish an
aarch64 Linux tarball and a wasm web package.

### `makepkg -si` prerequisites

A real `makepkg -si` cannot succeed from this repo alone yet. It requires,
none of which exist today:

1. **Published GitHub release artifacts.** `smelt` needs the tag tarball
   `v$pkgver.tar.gz` to exist upstream; `smelt-bin` needs the release asset
   `smelt-x86_64-unknown-linux-gnu.tar.gz` attached to a `v$pkgver` GitHub
   release. Neither release exists until the maintainer pushes the tag and
   uploads the binary.
2. **Real checksums.** The 64-zero placeholders in `smelt` and `smelt-bin`
   must be replaced by running `scripts/aur-update.sh <pkg> <version>`,
   which downloads the tarball and rewrites `sha256sums`. A build against
   the placeholder sums fails at the integrity check.
3. **A build toolchain.** `smelt` and `smelt-git` build from source, so they
   need `cargo` plus `git` (the test suite drives real git fixtures, and
   `makepkg` runs `check()` by default) plus the base-devel group on the
   build machine; `smelt-bin` needs no toolchain, only `--nodeps` is enough
   to test it before the release asset exists.
4. **A non-root user.** `makepkg` refuses to run as root.

Until 1 and 2 are satisfied, local `makepkg -s` on `smelt`/`smelt-bin` can
only be expected to fail at source retrieval / integrity, not to build.
`smelt-git` is the only package whose source resolves without a release
(it clones HEAD), so it is the one whose source can be fetched today; its
`pkgver()` falls back to `2.0.0.r0.g<commit>` when the clone carries no tag.
`build()` and `package()` in all three PKGBUILDs agree on cargo's default
target directory (no `CARGO_TARGET_DIR` override), which the packaging tests
pin; a full `makepkg -s` still needs network access to GitHub and the real
release artifacts listed above.

## Placeholder checksums

`smelt` and `smelt-bin` ship real sha256 checksums after the first tagged
release; until then the PKGBUILDs carry clearly marked placeholder
checksums that must be replaced by `scripts/aur-update.sh` before any
publish. `smelt-git` always uses `SKIP` for the VCS source.
