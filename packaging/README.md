# Packaging

Templates for the stores Metamorphosis ships to. `scripts/render_packaging.py`
fills them in for a release with the version, owner, download URLs and the
SHA-256 of each release asset. The release workflow runs it and publishes
wherever credentials exist.

| Store | Files | Published by | Secret |
|---|---|---|---|
| winget (`winget install morph`) | `winget/*.yaml` | `release.yml` → `winget` job (Komac PR to microsoft/winget-pkgs) | `WINGET_TOKEN` |
| Flathub (`flatpak install flathub io.github.OWNER.morph`) | `flatpak/`, `linux/` | by hand: the first submission is a PR to flathub/flathub, and later updates go to the app repo Flathub creates | none |
| AUR (`yay -S morph-bin`) | `aur/PKGBUILD` | `release.yml` → `aur` job | `AUR_SSH_PRIVATE_KEY` |
| AppImage with zsync delta updates | `scripts/appimage_zsync.sh` | `release.yml` (Linux build) | none |
| Signed MSI and NSIS installers | `release.yml` (Windows build) | | `WINDOWS_CERTIFICATE` (base64 PFX), `WINDOWS_CERTIFICATE_PASSWORD` |

Without the secrets, those jobs skip publishing. The rendered manifests are
still attached to the release as `packaging.zip`, so they can be submitted
by hand.

Render locally:

```sh
python3 scripts/render_packaging.py --version 1.0.0 --owner my-user --assets dist/ --out target/packaging
```

Placeholders: `{OWNER}` (GitHub user or org), `{APP_ID}` (`io.github.<owner>.morph`),
`{VERSION}`, `{DATE}`, `{SHA256_MSI}`, `{SHA256_NSIS}`, `{SHA256_DEB}`, `{SHA256_CLI}`.
