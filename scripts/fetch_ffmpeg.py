#!/usr/bin/env python3
"""Download the pinned LGPL FFmpeg build into vendor/ffmpeg/<os>/ (checksum-verified)
for bundling as a sidecar in the installers. Keep in sync with morph-updater's catalog.

    python3 scripts/fetch_ffmpeg.py
"""
import hashlib
import io
import lzma
import pathlib
import platform
import sys
import tarfile
import urllib.request
import zipfile

BASE = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-10-18-13-01/"
BUILDS = {
    "windows": ("ffmpeg-n8.1.3-18-g46d899d7a0-win64-lgpl-shared-8.1.zip",
                "eb9346bdb3312d51ce5bfcf78041fd17e3db91f9fff9446d0d758406b85d2a51"),
    "linux": ("ffmpeg-n8.1.3-18-g46d899d7a0-linux64-lgpl-shared-8.1.tar.xz",
              "2fb1306739c117c7c0e3dbe8bb2fd968ee3976323c6537c6151337b05ef3ea07"),
}
ROOT = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    os_name = sys.argv[1] if len(sys.argv) > 1 else ("windows" if platform.system() == "Windows" else "linux")
    asset, sha = BUILDS[os_name]
    dest = ROOT / "vendor" / "ffmpeg" / os_name
    marker = dest / ".complete"
    if marker.exists():
        print(f"ffmpeg already in {dest}")
        return 0
    print(f"downloading {asset}")
    data = urllib.request.urlopen(BASE + asset).read()
    if hashlib.sha256(data).hexdigest() != sha:
        print("checksum mismatch")
        return 1
    dest.mkdir(parents=True, exist_ok=True)
    prefix = asset.rsplit(".", 2 if asset.endswith(".tar.xz") else 1)[0] + "/"
    if os_name == "windows":
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            for n in z.namelist():
                rel = n[len(prefix):]
                if (rel.startswith("bin/") and not rel.endswith("/") and "ffplay" not in rel) or rel == "LICENSE.txt":
                    (dest / pathlib.Path(rel).name).write_bytes(z.read(n))
    else:
        with tarfile.open(fileobj=io.BytesIO(lzma.decompress(data))) as t:
            members = {m.name[len(prefix):]: m for m in t.getmembers() if m.name.startswith(prefix)}
            for rel, m in members.items():
                if not m.isfile():
                    continue
                if rel in ("bin/ffmpeg", "bin/ffprobe", "LICENSE.txt") or (rel.startswith("lib/") and ".so." in rel):
                    out = dest / rel
                    if rel.startswith("lib/"):
                        # libavcodec.so.62.28.103 → libavcodec.so.62 (the soname ffmpeg asks for)
                        name = pathlib.Path(rel).name
                        stem, ver = name.split(".so.")
                        out = dest / "lib" / f"{stem}.so.{ver.split('.')[0]}"
                    out.parent.mkdir(parents=True, exist_ok=True)
                    out.write_bytes(t.extractfile(m).read())
                    if rel.startswith("bin/"):
                        out.chmod(0o755)
    marker.write_text(asset)
    print(f"ffmpeg ready in {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
