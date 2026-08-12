#!/usr/bin/env bash
#
# Downloads the ffmpeg/ffprobe sidecars into src-tauri/binaries/, named with the
# Rust target triple Tauri expects.
#
#   ./scripts/fetch-ffmpeg.sh            # this machine's platform
#   ./scripts/fetch-ffmpeg.sh windows    # cross-fetch for a specific platform
#   ./scripts/fetch-ffmpeg.sh all
#
# Builds come from https://github.com/BtbN/FFmpeg-Builds.
#
# Coldmill ships the GPL build: it is the one that carries x264/x265, and
# without them there is no usable H.264 output. That is why the app itself is
# GPL-3.0. Set FFMPEG_LICENSE=lgpl for a permissively licensed build — you then
# lose H.264/H.265 encoding and must adjust H264_ENCODER in
# src-tauri/src/presets.rs.
set -euo pipefail

FFMPEG_LICENSE="${FFMPEG_LICENSE:-gpl}"
# Pinned to a release line rather than `master`, which is rebuilt daily and
# would make two builds of the same tag differ. Use FFMPEG_BUILD=master for the
# bleeding edge, or n7.1 for the older LTS-ish line.
FFMPEG_BUILD="${FFMPEG_BUILD:-n8.1}"
BASE_URL="${FFMPEG_BASE_URL:-https://github.com/BtbN/FFmpeg-Builds/releases/download/latest}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$ROOT_DIR/src-tauri/binaries"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

log() { printf '\033[36m==>\033[0m %s\n' "$*"; }
die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

download() {
  local url="$1" out="$2"
  log "downloading $(basename "$url")"
  if command -v curl >/dev/null 2>&1; then
    curl -fSL --retry 3 --progress-bar -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "$out" "$url"
  else
    die "neither curl nor wget is available"
  fi
}

# Git Bash has no unzip, so try everything a Windows box might have.
extract_zip() {
  local archive="$1" target="$2"
  mkdir -p "$target"
  if command -v unzip >/dev/null 2>&1; then
    unzip -q "$archive" -d "$target"
  elif command -v 7z >/dev/null 2>&1; then
    7z x -y -o"$target" "$archive" >/dev/null
  elif tar --version 2>/dev/null | grep -qi bsdtar; then
    tar -xf "$archive" -C "$target"
  elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command \
      "Expand-Archive -LiteralPath '$(cygpath -w "$archive" 2>/dev/null || echo "$archive")' -DestinationPath '$(cygpath -w "$target" 2>/dev/null || echo "$target")' -Force"
  else
    die "no way to extract a zip file (install unzip or 7z)"
  fi
}

install_pair() {
  local extracted="$1" triple="$2" suffix="$3"
  for tool in ffmpeg ffprobe; do
    local src
    src="$(find "$extracted" -type f -name "${tool}${suffix}" -print -quit)"
    [ -n "$src" ] || die "${tool}${suffix} not found in the archive"
    install -m 0755 "$src" "$DEST_DIR/${tool}-${triple}${suffix}"
    log "installed ${tool}-${triple}${suffix}"
  done
}

# Release-line archives repeat the version at the end (…-gpl-8.1.zip);
# master builds do not.
archive_stem() {
  local platform="$1" tail=""
  case "$FFMPEG_BUILD" in
    n*) tail="-${FFMPEG_BUILD#n}" ;;
  esac
  echo "ffmpeg-${FFMPEG_BUILD}-latest-${platform}-${FFMPEG_LICENSE}${tail}"
}

fetch_windows() {
  local name
  name="$(archive_stem win64)"
  download "$BASE_URL/${name}.zip" "$WORK_DIR/win.zip"
  extract_zip "$WORK_DIR/win.zip" "$WORK_DIR/win"
  install_pair "$WORK_DIR/win" "x86_64-pc-windows-msvc" ".exe"
}

fetch_linux() {
  local name
  name="$(archive_stem linux64)"
  download "$BASE_URL/${name}.tar.xz" "$WORK_DIR/linux.tar.xz"
  mkdir -p "$WORK_DIR/linux"
  tar -xJf "$WORK_DIR/linux.tar.xz" -C "$WORK_DIR/linux"
  install_pair "$WORK_DIR/linux" "x86_64-unknown-linux-gnu" ""
}

detect_platform() {
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN* | Windows_NT) echo windows ;;
    Linux) echo linux ;;
    Darwin) die "macOS is not supported yet — see the roadmap in README.md" ;;
    *) die "unknown platform: $(uname -s)" ;;
  esac
}

main() {
  mkdir -p "$DEST_DIR"
  local platform="${1:-$(detect_platform)}"
  log "license=$FFMPEG_LICENSE build=$FFMPEG_BUILD platform=$platform"

  case "$platform" in
    windows) fetch_windows ;;
    linux) fetch_linux ;;
    all)
      fetch_windows
      fetch_linux
      ;;
    *) die "usage: $0 [windows|linux|all]" ;;
  esac

  log "done — binaries are in src-tauri/binaries/"
}

main "$@"
