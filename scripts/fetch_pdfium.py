#!/usr/bin/env python3
"""Download the pinned PDFium build for this OS into vendor/pdfium/<os>/ (checksum-verified),
and copy the library next to the dev/test binaries in target/<profile>/.

The app bundles the library as a resource (see tauri.<os>.conf.json); `morph` looks for it
next to its own executable. Run once after cloning:  python3 scripts/fetch_pdfium.py
"""
import hashlib
import io
import pathlib
import platform
import shutil
import sys
import tarfile
import urllib.request

VERSION = "chromium/8100"
BUILDS = {
    "windows": ("pdfium-win-x64.tgz", "bin/pdfium.dll", "51621bcc1d9be9ea448f4b84b0516862b928e3968d3fca899af1a6d6e2848d6d"),
    "linux": ("pdfium-linux-x64.tgz", "lib/libpdfium.so", "1c1894a12d4d96ee01400c3874444126254180c5c7d2ab628576b67f8120320a"),
}
ROOT = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    os_name = "windows" if platform.system() == "Windows" else "linux"
    if len(sys.argv) > 1:
        os_name = sys.argv[1]
    asset, lib, sha = BUILDS[os_name]
    dest = ROOT / "vendor" / "pdfium" / os_name
    lib_name = pathlib.Path(lib).name
    if not (dest / lib_name).exists():
        url = f"https://github.com/bblanchon/pdfium-binaries/releases/download/{VERSION}/{asset}"
        print(f"downloading {url}")
        data = urllib.request.urlopen(url).read()
        digest = hashlib.sha256(data).hexdigest()
        if digest != sha:
            print(f"checksum mismatch: got {digest}, want {sha}")
            return 1
        dest.mkdir(parents=True, exist_ok=True)
        with tarfile.open(fileobj=io.BytesIO(data)) as tar:
            for member in tar.getmembers():
                if not member.isfile():
                    continue
                if member.name == lib:
                    out = dest / lib_name
                elif member.name == "LICENSE":
                    out = dest / "PDFIUM-LICENSE.txt"
                elif member.name.startswith("licenses/") and "/" not in member.name[9:]:
                    out = dest / "licenses" / member.name[9:]
                else:
                    continue
                out.parent.mkdir(parents=True, exist_ok=True)
                out.write_bytes(tar.extractfile(member).read())
    for profile in ("debug", "release"):
        for target_dir in [ROOT / "target", *(ROOT / "target").glob("*")]:
            out = target_dir / profile
            if out.is_dir():
                shutil.copy2(dest / lib_name, out / lib_name)
                deps = out / "deps"
                if deps.is_dir():
                    shutil.copy2(dest / lib_name, deps / lib_name)
    print(f"pdfium {VERSION} ready in {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
