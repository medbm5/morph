#!/usr/bin/env python3
"""Fill the packaging/ templates for a release.

    render_packaging.py --version 1.0.0 --owner my-user --assets dist/ --out target/packaging

`--assets` is a folder holding the release files (MSI, NSIS setup, .deb, CLI zip);
their SHA-256 sums go into the manifests. Missing assets leave a visible
`MISSING-<name>` marker and a non-zero exit unless `--allow-missing`.
"""

import argparse
import datetime
import hashlib
import pathlib
import shutil
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TEMPLATES = ROOT / "packaging"


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--owner", required=True)
    ap.add_argument("--assets", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--date", default=datetime.date.today().isoformat())
    ap.add_argument("--allow-missing", action="store_true")
    a = ap.parse_args()

    v = a.version.removeprefix("v")
    owner = a.owner
    # Flatpak ids allow letters, digits and underscores per segment.
    app_id = "io.github." + owner.replace("-", "_") + ".morph"
    assets = {
        "SHA256_MSI": f"Metamorphosis_{v}_x64_en-US.msi",
        "SHA256_NSIS": f"Metamorphosis_{v}_x64-setup.exe",
        "SHA256_DEB": f"Metamorphosis_{v}_amd64.deb",
        "SHA256_CLI": "morph-linux-x86_64.zip",
    }
    values = {"VERSION": v, "OWNER": owner, "APP_ID": app_id, "DATE": a.date}
    missing = []
    for key, name in assets.items():
        found = next(iter(sorted(a.assets.rglob(name))), None)
        if found:
            values[key] = sha256(found)
        else:
            values[key] = f"MISSING-{name}"
            missing.append(name)

    def render(text: str) -> str:
        for k, val in values.items():
            text = text.replace("{" + k + "}", val)
        return text

    if a.out.exists():
        shutil.rmtree(a.out)
    out = {
        "winget/morph.yaml": f"winget/{owner}.Metamorphosis.yaml",
        "winget/morph.installer.yaml": f"winget/{owner}.Metamorphosis.installer.yaml",
        "winget/morph.locale.en-US.yaml": f"winget/{owner}.Metamorphosis.locale.en-US.yaml",
        "flatpak/app.yml": f"flatpak/{app_id}.yml",
        "linux/metainfo.xml": "flatpak/metainfo.xml",
        "linux/metamorphosis.desktop": "flatpak/metamorphosis.desktop",
        "aur/PKGBUILD": "aur/PKGBUILD",
    }
    for src, dst in out.items():
        target = a.out / dst
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(render((TEMPLATES / src).read_text(encoding="utf-8")), encoding="utf-8", newline="\n")

    # AUR wants .SRCINFO next to the PKGBUILD (normally from makepkg --printsrcinfo).
    srcinfo = f"""pkgbase = morph-bin
\tpkgdesc = Drag a file, tap a key, drop it on the ring to convert or edit it. Offline.
\tpkgver = {v}
\tpkgrel = 1
\turl = https://github.com/{owner}/morph
\tarch = x86_64
\tlicense = MIT
\tlicense = Apache-2.0
\tdepends = webkit2gtk-4.1
\tdepends = gtk3
\tdepends = libayatana-appindicator
\tdepends = libxi
\tdepends = libxext
\toptdepends = gtk-layer-shell: pin the drop dock above windows on Wayland (KDE, wlroots)
\toptdepends = xdg-desktop-portal: global shortcut for the drop dock on Wayland
\toptdepends = libreoffice-fresh: Office to PDF and slide conversions
\tprovides = morph
\tprovides = metamorphosis
\tconflicts = morph
\tconflicts = metamorphosis
\tnoextract = morph-linux-x86_64.zip
\tsource_x86_64 = https://github.com/{owner}/morph/releases/download/v{v}/Metamorphosis_{v}_amd64.deb
\tsource_x86_64 = https://github.com/{owner}/morph/releases/download/v{v}/morph-linux-x86_64.zip
\tsha256sums_x86_64 = {values['SHA256_DEB']}
\tsha256sums_x86_64 = {values['SHA256_CLI']}

pkgname = morph-bin
"""
    (a.out / "aur/.SRCINFO").write_text(srcinfo, encoding="utf-8", newline="\n")

    print(f"rendered packaging for v{v} ({app_id}) into {a.out}")
    if missing:
        print("missing assets: " + ", ".join(missing), file=sys.stderr)
        return 0 if a.allow_missing else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
