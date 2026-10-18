#!/usr/bin/env bash
# Embed update information in the AppImage and write its .zsync file, so
# AppImageUpdate (and tools built on it) download only the changed blocks.
#
#   scripts/appimage_zsync.sh <path/to/Metamorphosis_X_amd64.AppImage> <owner>
#
# The update channel follows the latest GitHub release:
#   gh-releases-zsync|<owner>|morph|latest|Metamorphosis_*_amd64.AppImage.zsync
set -euo pipefail

appimage=$(readlink -f "$1")
owner=$2
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

tool="$work/appimagetool"
# Pinned appimagetool release.
curl -fsSL -o "$tool" \
  https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
echo "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  $tool" | sha256sum -c -
chmod +x "$tool"

cd "$work"
"$appimage" --appimage-extract >/dev/null
info="gh-releases-zsync|${owner}|morph|latest|Metamorphosis_*_amd64.AppImage.zsync"
# appimagetool writes <name>.zsync next to the output when update info is given.
ARCH=x86_64 "$tool" --no-appstream -u "$info" squashfs-root "$appimage.new" >/dev/null
mv "$appimage.new" "$appimage"
mv "$appimage.new.zsync" "$appimage.zsync" 2>/dev/null || mv ./*.zsync "$appimage.zsync"
sed -i "s|^Filename: .*|Filename: $(basename "$appimage")|" "$appimage.zsync"
echo "embedded: $info"
ls -l "$appimage" "$appimage.zsync"
