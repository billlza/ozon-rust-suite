#!/usr/bin/env bash
# Build the macOS DMG locally and attach it to the local-node release.
#
# CI only builds the Windows installer now (the GitHub macos-14 runner is billed
# at 10x, vs 2x for Windows and 1x for Linux). macOS is built here, on a real
# Mac, and uploaded to the same release + release-manifest.json — this replaces
# the deleted `macos-dmg` job in .github/workflows/local-node-release.yml.
#
# Run this AFTER the CI Windows release for the tag has published.
#
# Usage:
#   scripts/publish-macos-dmg.sh [version]
# Defaults the version to apps/local-node/src-tauri/tauri.conf.json.
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION="${1:-$(node -p "require('./apps/local-node/src-tauri/tauri.conf.json').version")}"
TAG="local-node-v${VERSION}"
DOWNLOADS_REPO="${OZON_DOWNLOADS_REPOSITORY:-billlza/ozon-rust-suite-downloads}"
BASE_URL="https://github.com/${DOWNLOADS_REPO}/releases/download/${TAG}"

echo "==> Releasing macOS DMG for ${TAG} -> ${DOWNLOADS_REPO}"

echo "==> Building macOS app bundle (this takes a few minutes) ..."
OZON_LOCAL_NODE_RELEASE_VERSION="${VERSION}" pnpm --dir apps/local-node tauri:build

# Search both the workspace target/ (where the Tauri DMG actually lands) and apps/.
# `|| true` keeps a missing dir + pipefail from aborting the whole script.
DMG_SRC="$(find target apps -path '*bundle/dmg/*.dmg' -print 2>/dev/null | head -n1 || true)"
if [ -z "${DMG_SRC}" ]; then
  echo "ERROR: no .dmg produced under */bundle/dmg/" >&2
  exit 1
fi
echo "    built: ${DMG_SRC}"

rm -rf dist/release
mkdir -p dist/release
DMG="dist/release/OzonRustLocal-aarch64.dmg"
cp "${DMG_SRC}" "${DMG}"
SHA="$(shasum -a 256 "${DMG}" | awk '{print $1}')"
echo "    sha256: ${SHA}"

echo "==> Uploading DMG to release ${TAG} ..."
gh release upload "${TAG}" "${DMG}" --repo "${DOWNLOADS_REPO}" --clobber

echo "==> Patching release-manifest.json with the macOS entry ..."
curl -fsSL "${BASE_URL}/release-manifest.json" -o dist/release/release-manifest.json
MAC_DMG_URL="${BASE_URL}/OzonRustLocal-aarch64.dmg" MAC_DMG_SHA256="${SHA}" python3 - <<'PY'
import json, os
from pathlib import Path

path = Path("dist/release/release-manifest.json")
manifest = json.loads(path.read_text())
manifest["macos_aarch64_dmg"] = {
    "url": os.environ["MAC_DMG_URL"],
    "sha256": os.environ["MAC_DMG_SHA256"],
}
path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
print("    manifest keys:", ", ".join(manifest.keys()))
PY
gh release upload "${TAG}" dist/release/release-manifest.json --repo "${DOWNLOADS_REPO}" --clobber

echo "==> Done. macOS DMG + updated manifest published to ${TAG}."
