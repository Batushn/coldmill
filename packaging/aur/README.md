# Arch packaging

`PKGBUILD` installs the released AppImage as `coldmill-bin`.

## Installing without the AUR

The AUR is only a place to publish this file — it is not needed to use it:

```bash
cd packaging/aur
makepkg -si
```

That downloads the AppImage from the GitHub release, unpacks its icon and
desktop entry, and installs everything under `/opt` with a `coldmill` launcher
on the path.

## Publishing to the AUR

Submission goes over SSH, so the account needs a key before anything will
push — an account without one fails silently.

1. Register at <https://aur.archlinux.org/register>
2. My Account → paste `~/.ssh/id_ed25519.pub` into **SSH Public Key**
3. Clone the (empty, auto-created) package repository and push:

```bash
git clone ssh://aur@aur.archlinux.org/coldmill-bin.git
cd coldmill-bin
cp ../PKGBUILD .
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Initial import of coldmill-bin 0.2.0"
git push
```

`.SRCINFO` is generated rather than written by hand, and the AUR rejects a push
without it.

## Keeping it current

Each release means bumping `pkgver`, resetting `pkgrel=1`, replacing the
checksum and regenerating `.SRCINFO`:

```bash
updpkgsums
makepkg --printsrcinfo > .SRCINFO
```

## About the checksum

`sha256sums` is `SKIP` here because this file was written before the release it
points at was published. Run `updpkgsums` once the tag is out — it fills in the
real hash, and shipping a package that verifies nothing is worse than shipping
none.
