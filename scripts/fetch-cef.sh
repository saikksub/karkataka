#!/usr/bin/env bash
# Download the CEF build pinned in cef.lock, in the layout the `cef` crate expects.
# Usage: scripts/fetch-cef.sh [dest]   (default: .cef in the repo; point CEF_PATH at dest)
# Outside the repo, set CEF_VERSION to the version in cef.lock.
# shortcut: the crate's own downloader fails because the CDN index.json is over its 10 MB limit; drop this script once cef-rs fixes that.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
version=${CEF_VERSION:-$(tr -d '[:space:]' < "$repo/cef.lock" 2>/dev/null || true)}
[[ -n $version ]] || { echo "no cef.lock next to this script; set CEF_VERSION" >&2; exit 1; }
dest="${1:-$repo/.cef}"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  platform=macosarm64 ;;
  Darwin-x86_64) platform=macosx64 ;;
  Linux-x86_64)  platform=linux64 ;;
  Linux-aarch64) platform=linuxarm64 ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

if [[ -f $dest/archive.json ]] && grep -q "cef_binary_${version}_" $dest/archive.json; then
  echo "CEF $version already in $dest"; exit 0
fi

name="cef_binary_${version}_${platform}_minimal"
url="https://cef-builds.spotifycdn.com/${name//+/%2B}.tar.bz2"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

echo "Downloading $name"
curl -fL --progress-bar -o "$tmp/cef.tar.bz2" "$url"
sha1=$(curl -fsSL "$url.sha1")
echo "$sha1  $tmp/cef.tar.bz2" | shasum -a 1 -c - >/dev/null || { echo "SHA1 mismatch" >&2; exit 1; }

tar -xjf "$tmp/cef.tar.bz2" -C "$tmp"
src="$tmp/$name"
mkdir -p "$(dirname "$dest")"; rm -rf "$dest"; mv "$src/Release" "$dest"
[[ $platform == macos* ]] || mv "$src/Resources/"* "$dest/"
mv "$src/CMakeLists.txt" "$src/cmake" "$src/include" "$src/libcef_dll" "$src/CREDITS.html" "$dest/"
printf '{"type":"minimal","name":"%s.tar.bz2","sha1":"%s"}\n' "$name" "$sha1" > "$dest/archive.json"
echo "CEF $version installed in $dest"
