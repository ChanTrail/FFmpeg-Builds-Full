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
# =============================================================================
set -euo pipefail
shopt -s nullglob

TAG="${1:?usage: $0 <release-tag> <archive-dir>}"
DIR="${2:?usage: $0 <release-tag> <archive-dir>}"

SERVER="${GITHUB_SERVER_URL:-https://github.com}"
REPO="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}"
BASE="$SERVER/$REPO/releases/download/$TAG"

# 全部目标平台清单 / All target platforms
PLATFORMS=(windows-x86_64 windows-arm64 linux-x86_64 linux-arm64 macos-arm64)

COMMIT_LINE_ZH="每个文件名中包含其构建所用的上游提交的前 10 位哈希。"
COMMIT_LINE_EN="The first 10 digits of the upstream commit each file was built from are part of its name."
if [[ -n "${FFMPEG_COMMIT:-}" ]]; then
  COMMIT_LINE_ZH="构建所用的上游提交：[\`${FFMPEG_COMMIT:0:10}\`](https://git.ffmpeg.org/gitweb/ffmpeg.git/commit/$FFMPEG_COMMIT)，文件名中包含其前 10 位哈希。"
  COMMIT_LINE_EN="Built from upstream commit [\`${FFMPEG_COMMIT:0:10}\`](https://git.ffmpeg.org/gitweb/ffmpeg.git/commit/$FFMPEG_COMMIT); its first 10 digits are part of every file name."
fi

cat <<EOF
# FFmpeg master 自动构建 / auto-build

自动构建上游 [FFmpeg](https://git.ffmpeg.org/gitweb/ffmpeg.git) \`master\` 分支。$COMMIT_LINE_ZH
<br>Automated build of upstream [FFmpeg](https://git.ffmpeg.org/gitweb/ffmpeg.git) \`master\`. $COMMIT_LINE_EN
EOF

# 平台名作为表格列；行是四个产物变体（静态/共享/调试/共享+调试）。
# 单元格只放压缩包本身一个链接——大小与 .sha256 不进表（校验文件
# 的用法由表下说明段交代）/ platform names as table columns; rows
# are the four artifact variants (static/shared/debug/shared+debug).
# Each cell carries just the archive link — size and .sha256 stay out
# of the table (the paragraph below explains checksum usage)
# sep 必须给行标签列也放一格 ---：表头是 产物+5平台=6 列，分隔行少
# 一格就与表头列数不符，cmark-gfm 按"列数不符不识别为 table"把整表
# 渲染成裸管道文本（GFM 硬规则，与空行无关；实证：线上页加了空行
# 后仍输出 <p>| 产物——本 bug 自旧版脚本就一直在）/ sep must seed a
# --- cell for the row-label column too: the header has Artifact+5
# platforms = 6 cells, and a delimiter row one cell short mismatches
# the header count — cmark-gfm then refuses to recognize the table
# and renders it as raw pipe text (a hard GFM rule, orthogonal to
# blank lines; empirical: the live page still emitted <p>| 产物 after
# the blank line landed — this bug predates the rewrite)
header="| 产物 Artifact |" sep="|---|"
declare -A ROWS=()
# (变体后缀, 行标签) / (variant suffix, row label)
VARIANTS=("" "-shared" "-debug" "-shared-debug")
LABELS=("**静态 Static**" "**共享 Shared**" "**调试 Debug**" "**共享+调试 Shared+Debug**")
for vi in "${!VARIANTS[@]}"; do ROWS[$vi]="| ${LABELS[$vi]} |"; done
for p in "${PLATFORMS[@]}"; do
  header="$header $p |"; sep="$sep---|"
  for vi in "${!VARIANTS[@]}"; do
    v="${VARIANTS[$vi]}"
    found=("$DIR"/*-"$p$v".7z "$DIR"/*-"$p$v".tar.xz)
    if [[ ${#found[@]} -ne 1 ]]; then
      echo "::error::expected exactly one archive for platform '$p' variant '$v' in $DIR, found ${#found[@]}" >&2
      exit 1
    fi
    f="${found[0]}"
    name="$(basename "$f")"
    ROWS[$vi]="${ROWS[$vi]} [$name]($BASE/$name) |"
  done
done
# 表格与介绍段之间必须空一行——GitHub 的 cmark-gfm 只把空行分隔出
# 的管道块识别为 table；紧贴段落的表格整表留在段落里渲染成裸文本
# （cmark-gfm#141；实证：本仓 release 页产物表炸成字面管道符，而带
# 空行的完整度表正常渲染）/ a blank line MUST separate the table from
# the intro paragraph — GitHub's cmark-gfm only recognizes a pipe block
# as a table when a blank line detaches it; a paragraph-adjacent table
# stays inside the paragraph and renders as raw text (cmark-gfm#141;
# empirical: this repo's release page rendered the artifact table as
# literal pipes while the blank-line-separated completeness table
# rendered fine)
printf '\n%s\n%s\n' "$header" "$sep"
for vi in "${!VARIANTS[@]}"; do printf '%s\n' "${ROWS[$vi]}"; done

cat <<'EOF'

每个压缩包都有同名的 `.sha256` 文件：把两者放在同一目录，执行 `sha256sum -c <压缩包>.sha256` 即可校验（macOS：`shasum -a 256 -c <压缩包>.sha256`）。包内的 `BUILDINFO.info` 列出了该平台启用的全部配置选项，以及构建失败或被放弃的库。
<br>Every archive has a same-named `.sha256` file; put both in one folder and run `sha256sum -c <archive>.sha256` (macOS: `shasum -a 256 -c <archive>.sha256`). `BUILDINFO.info` inside the archive lists every configure option of that platform and the libraries that failed or were dropped.

EOF
