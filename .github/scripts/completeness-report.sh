#!/usr/bin/env bash
# =============================================================================
# FFmpeg-Builds-Full 库完整度报告生成器
# FFmpeg-Builds-Full library completeness report generator
#
# 打印一份中英双语 Markdown 报告：对 FFmpeg configure 认识的每个外部库，
# 显示各平台构建里有没有它，外加各平台构建失败/被放弃的组件。
# Print a bilingual (Chinese / English) Markdown report that shows, for every
# external library FFmpeg's configure knows about, whether each platform's
# build has it, plus what failed to build on each platform.
#
# 用法 / Usage: completeness-report.sh <ffmpeg-source-dir> <archives-dir>
#
# <ffmpeg-source-dir> 只需要 FFmpeg 的 `configure`（以 --help 运行）。
# <archives-dir> 存放每个平台的 ffmpeg-*-<platform>.{7z,tar.xz}；构建
# 报告在包内 <name>/BUILDINFO.info，内容是 `ffmpeg -buildconf` 的输出。
# <ffmpeg-source-dir> only needs FFmpeg's `configure` (it is run with --help).
# <archives-dir> holds ffmpeg-*-<platform>.{7z,tar.xz} for every platform;
# the build report lives inside each archive as <name>/BUILDINFO.info and
# contains the output of `ffmpeg -buildconf`.
# =============================================================================
set -euo pipefail
shopt -s nullglob

SRC="${1:?usage: $0 <ffmpeg-source-dir> <archives-dir>}"
DIR="${2:?usage: $0 <ffmpeg-source-dir> <archives-dir>}"

# 全部目标平台清单 / All target platforms
PLATFORMS=(windows-x86_64 windows-arm64 linux-x86_64 linux-arm64 macos-arm64)
# 有意排除的库：非自由——含它们的二进制不可再分发。
# Libraries that are deliberately left out: they are non-free, so a binary that
# contains them could not be redistributed.
NONFREE=" libfdk-aac libnpp decklink "

[[ -f "$SRC/configure" ]] || { echo "::error::$SRC/configure not found" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# 可选外部库：`configure --help` 的 "External library support" 一节里的
# "--enable-X" 选项。
# The opt-in external libraries: "--enable-X" options of the section
# "External library support" of `configure --help`.
(cd "$SRC" && sh ./configure --help) |
  awk '/^External library support:/ { f = 1; next }
       /^Toolchain options:/ { f = 0 }
       f && /^  --enable-/ { sub(/^  --enable-/, "", $1); print $1 }' |
  sort -u >"$tmp/candidates.txt"
total="$(wc -l <"$tmp/candidates.txt" | tr -d ' ')"
[[ "$total" -gt 0 ]] || { echo "::error::no external libraries found in configure --help" >&2; exit 1; }

# 每个平台启用的选项：从压缩包内抽出 BUILDINFO.info 到 stdout
# （.7z 走 7z x -so，.tar.xz 走 tar -xO；两者都支持通配成员名）。
# The enabled options of each platform: BUILDINFO.info is pulled from inside
# the archive straight to stdout (7z x -so for .7z, tar -xO for .tar.xz;
# both accept a wildcard member name).
for p in "${PLATFORMS[@]}"; do
  found=("$DIR"/*-"$p".7z "$DIR"/*-"$p".tar.xz)
  [[ ${#found[@]} -eq 1 ]] || { echo "::error::expected one archive for $p in $DIR, found ${#found[@]}" >&2; exit 1; }
  f="${found[0]}"
  case "$f" in
    *.7z)
      sevenz="$(command -v 7zz || command -v 7z)"
      "$sevenz" x -so "$f" '*/BUILDINFO.info' >"$tmp/$p.info"
      ;;
    *.tar.xz)
      tar -xOf "$f" --wildcards '*/BUILDINFO.info' >"$tmp/$p.info"
      ;;
  esac
  [[ -s "$tmp/$p.info" ]] || { echo "::error::no BUILDINFO.info inside $f" >&2; exit 1; }
  grep -oE -- '--enable-[A-Za-z0-9_-]+' "$tmp/$p.info" | sed 's/^--enable-//' | sort -u >"$tmp/$p.enabled"
done

# count <platform>：该平台启用了多少个候选库
# Number of candidates the platform has
count() {
  comm -12 "$tmp/candidates.txt" "$tmp/$1.enabled" | wc -l | tr -d ' '
}

echo "## 库完整度 / Library completeness"
echo
echo "FFmpeg 的 \`configure\` 共列出 **$total** 个需要显式启用的外部库。下表显示每个平台实际启用了哪些。"
echo "<br>FFmpeg's \`configure\` lists **$total** external libraries that have to be enabled explicitly. The table shows which of them each platform has."
echo
echo "| 平台 Platform | 已启用 Enabled |"
echo "|:--|--:|"
for p in "${PLATFORMS[@]}"; do
  echo "| $p | $(count "$p") / $total |"
done
echo
echo "<details><summary>逐库明细 / Per-library table</summary>"
echo
printf '| 库 Library |'
for p in "${PLATFORMS[@]}"; do printf ' %s |' "$p"; done
echo
printf '|:--|'
for _ in "${PLATFORMS[@]}"; do printf ':-:|'; done
echo
while IFS= read -r lib; do
  printf '| %s |' "$lib"
  for p in "${PLATFORMS[@]}"; do
    if grep -qxF -- "$lib" "$tmp/$p.enabled"; then
      printf ' ✅ |'
    elif [[ "$NONFREE" == *" $lib "* ]]; then
      printf ' ⛔ |'
    else
      printf ' ❌ |'
    fi
  done
  echo
done <"$tmp/candidates.txt"
echo
echo "⛔ = 非自由库，为保证可再分发而有意排除 / non-free, deliberately excluded so the binaries stay redistributable."
echo
echo "</details>"
echo

for p in "${PLATFORMS[@]}"; do
  failed="$(sed -n 's/^[[:space:]]\{1,\}\([^[:space:]]\{1,\}\) failed$/\1/p' "$tmp/$p.info" | tr '\n' ' ')"
  dropped="$(sed -n 's/^Dropped by configure : *//p' "$tmp/$p.info" | head -n1)"
  missing="$(sed -n 's/^MSYS2 packages not available\/installed : *//p' "$tmp/$p.info" | head -n1)"
  stages="$(sed -n 's/^Failed stages  : *//p' "$tmp/$p.info" | head -n1)"
  [[ -n "${failed// /}${dropped// /}${missing// /}${stages// /}" ]] || continue
  echo "<details><summary>$p: 未能构建的组件 / what did not make it</summary>"
  echo
  [[ -z "${failed// /}" ]] || echo "- 额外库构建失败 / extra libraries that failed to build: \`${failed% }\`"
  [[ -z "${dropped// /}" ]] || echo "- configure 阶段被放弃的选项 / options dropped at configure time: \`${dropped% }\`"
  [[ -z "${missing// /}" ]] || echo "- 该架构没有的 MSYS2 软件包 / MSYS2 packages not available: \`${missing% }\`"
  [[ -z "${stages// /}" ]] || echo "- 构建失败的库阶段 / library stages that failed: \`${stages% }\`"
  echo
  echo "</details>"
  echo
done
