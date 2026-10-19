# docs/AUR.md -- AUR publish runbook

How smelt releases reach the Arch User Repository. Three packages:

| Package | Source | Use case |
| --- | --- | --- |
| `smelt` | tag tarball, built with cargo | most users |
| `smelt-bin` | prebuilt static release binary | no Rust toolchain needed |
| `smelt-git` | git HEAD | testers |

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

## Placeholder checksums

`smelt` and `smelt-bin` ship real sha256 checksums after the first tagged
release; until then the PKGBUILDs carry clearly marked placeholder
checksums that must be replaced by `scripts/aur-update.sh` before any
publish. `smelt-git` always uses `SKIP` for the VCS source.
