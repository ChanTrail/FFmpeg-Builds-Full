#!/usr/bin/env bash
# =============================================================================
# FFmpeg-Builds-Full 发布说明生成器
# FFmpeg-Builds-Full release notes generator
#
# 为目录中找出的各平台压缩包打印中英双语 Markdown 发布说明。
# Print bilingual (Chinese / English) Markdown release notes for the archives
# found in a directory.
#
# 用法 / Usage: release-notes.sh <release-tag> <archive-dir>
#
# 需要 GITHUB_REPOSITORY；有 GITHUB_SERVER_URL 时使用它。任一平台缺压缩包
# 即非零退出：发布是全有或全无。
# Needs GITHUB_REPOSITORY; uses GITHUB_SERVER_URL when set. Exits non-zero if
# any platform has no archive: releases are all-or-nothing.
#
# 可选环境变量 / Optional environment:
#   FFMPEG_COMMIT  构建所用的上游完整提交
#                  full upstream commit the builds were made from
#   FFMPEG_HASH    其前 10 位（也会从文件名读出）
#                  its first 10 digits (also read from the file names)
#   FFMPEG_SRC     上游 FFmpeg 检出；追加库完整度报告
#                  an upstream FFmpeg checkout; adds the library completeness report
# =============================================================================
set -euo pipefail
shopt -s nullglob

TAG="${1:?usage: $0 <release-tag> <archive-dir>}"
DIR="${2:?usage: $0 <release-tag> <archive-dir>}"

SERVER="${GITHUB_SERVER_URL:-https://github.com}"
REPO="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}"
BASE="$SERVER/$REPO/releases/download/$TAG"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 全部目标平台清单 / All target platforms
PLATFORMS=(windows-x86_64 windows-arm64 linux-x86_64 linux-arm64 macos-arm64)

# human_size <bytes>：把字节数格式化为人类可读（如 "123.4MiB"）
# Format a byte count as human-readable (e.g. "123.4MiB")
human_size() {
  local bytes="$1"
  numfmt --to=iec-i --suffix=B --format='%.1f' "$bytes" 2>/dev/null || echo "${bytes} B"
}

COMMIT_LINE_ZH="每个文件名中包含其构建所用的上游提交的前 10 位哈希。"
COMMIT_LINE_EN="The first 10 digits of the upstream commit each file was built from are part of its name."
if [[ -n "${FFMPEG_COMMIT:-}" ]]; then
  COMMIT_LINE_ZH="构建所用的上游提交：[\`${FFMPEG_COMMIT:0:10}\`](https://github.com/FFmpeg/FFmpeg/commit/$FFMPEG_COMMIT)，文件名中包含其前 10 位哈希。"
  COMMIT_LINE_EN="Built from upstream commit [\`${FFMPEG_COMMIT:0:10}\`](https://github.com/FFmpeg/FFmpeg/commit/$FFMPEG_COMMIT); its first 10 digits are part of every file name."
fi

cat <<EOF
# FFmpeg master 自动构建 / auto-build

自动构建上游 [FFmpeg](https://github.com/FFmpeg/FFmpeg) \`master\` 分支。$COMMIT_LINE_ZH
<br>Automated build of upstream [FFmpeg](https://github.com/FFmpeg/FFmpeg) \`master\`. $COMMIT_LINE_EN
EOF

# 平台名作为表格列；行是产物种类。单元格把每个产物名链接到它的下载——
# 与 deps 发布同布局。
# Platform names as table columns; rows are the artifact kinds. Cells link
# each artifact name to its download — same layout as the deps release.
header="| 产物 Artifact |" sep="|" arow="| **压缩包 Archive** |"
srow="| **校验 SHA-256** |"
for p in "${PLATFORMS[@]}"; do
  found=("$DIR"/*-"$p".7z "$DIR"/*-"$p".tar.xz)
  if [[ ${#found[@]} -ne 1 ]]; then
    echo "::error::expected exactly one archive for platform '$p' in $DIR, found ${#found[@]}" >&2
    exit 1
  fi
  f="${found[0]}"
  name="$(basename "$f")"
  size="$(human_size "$(wc -c <"$f" | tr -d ' ')")"
  header="$header $p |"; sep="$sep---|"
  arow="$arow [$name]($BASE/$name)<br>$size |"
  srow="$srow [$name.sha256]($BASE/$name.sha256) |"
done
printf '%s\n%s\n%s\n%s\n' "$header" "$sep" "$arow" "$srow"

cat <<'EOF'

每个压缩包都有同名的 `.sha256` 文件：把两者放在同一目录，执行 `sha256sum -c <压缩包>.sha256` 即可校验（macOS：`shasum -a 256 -c <压缩包>.sha256`）。包内的 `BUILDINFO.txt` 列出了该平台启用的全部配置选项，以及构建失败或被放弃的库。
<br>Every archive has a same-named `.sha256` file; put both in one folder and run `sha256sum -c <archive>.sha256` (macOS: `shasum -a 256 -c <archive>.sha256`). `BUILDINFO.txt` inside the archive lists every configure option of that platform and the libraries that failed or were dropped.

## 关于这些构建 / About these builds

- **许可证 License**：GPLv3（`--enable-gpl --enable-version3`）。链接了该平台上所有与 GPLv3 兼容的外部库；fdk-aac 等非自由库被刻意排除，因此二进制文件可以再分发。
  <br>GPLv3 (`--enable-gpl --enable-version3`). Every external library that is compatible with GPLv3 and available for the platform is linked in; non-free libraries such as fdk-aac are deliberately left out so the binaries stay redistributable.
- **原生构建 Native builds**：五个平台都在各自系统和架构的 runner 上原生编译，没有交叉编译。
  <br>All five platforms are compiled natively on a runner of their own OS and CPU architecture; nothing is cross-compiled.
  - Windows x86_64：MSYS2 UCRT64；Windows arm64：MSYS2 CLANGARM64。要求 Windows 10 及以上。 / Windows 10 or newer.
  - Linux：Ubuntu（x86_64 与 arm64 runner），库配方为本仓 Rust 驱动内置配方。要求 glibc >= 2.39。 / Library recipes are this repo's Rust-driver built-ins. Needs glibc >= 2.39.
  - macOS：同一 Rust 驱动构建，最低 macOS 14。二进制文件为 ad-hoc 签名，未经公证：下载后请执行 `xattr -dr com.apple.quarantine <解压目录>`。 / Built by the same Rust driver, minimum macOS 14. The binaries are ad-hoc signed but not notarized: after downloading, run `xattr -dr com.apple.quarantine <extracted folder>`.
- **链接方式 Linking**：静态链接，无需安装任何共享库。
  <br>Static, no shared libraries to install.
- **发布条件 Publishing**：只有五个平台全部构建并通过冒烟测试后才会发布。
  <br>A release is only published when all five platforms were built and passed the smoke test.

EOF

if [[ -n "${FFMPEG_SRC:-}" ]]; then
  if ! bash "$HERE/completeness-report.sh" "$FFMPEG_SRC" "$DIR"; then
    echo "（库完整度报告生成失败，详见各平台的构建报告。 / The library completeness report could not be generated; see each platform's build report.）"
  fi
fi
