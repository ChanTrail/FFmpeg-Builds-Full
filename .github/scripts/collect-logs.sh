#!/usr/bin/env bash
# 日志收集：把驱动散落在工作区之外的产物收进 <logs_dir>，供
# upload-artifact 打包上传。
# Log collection: gather the driver's scattered out-of-workspace output
# into <logs_dir> so upload-artifact can pick it up.
#
# 包内布局 / bundle layout:
#   file-tree.txt   两个根的全量文件清单——行即包内路径，一眼对应实物
#                   full manifest of both roots — each line equals the
#                   in-bundle path, mapping straight to the bundled copy
#   errors.txt      本包出错索引：失败配方 → 其 .log；configure 丢包 →
#                   其 config-log；以及全部详细日志清单——进包先看这里
#                   failure index of the bundle: failed recipes -> their
#                   .log, configure drops -> their config-log, and every
#                   detailed log captured — open this first
#   <name>/         work 下的 *.txt、*.log，外加 failed/ 失败现场整树；
#                   <name> 恒为 ffmpeg-<platform>（如 ffmpeg-macos-arm64），
#                   各平台根目录名一致对齐
#                   every *.txt / *.log under <work>, plus the whole
#                   failed/ salvage tree; <name> is always
#                   ffmpeg-<platform> (e.g. ffmpeg-macos-arm64) so the
#                   root dir reads the same on every platform
#   <name>/.extras/ extras 状态目录全量（.ok/done 无扩展名标记也进）
#                   the extras state dir wholesale (extensionless
#                   .ok/done markers included)
#
# 用法 / usage: collect-logs.sh <logs_dir> <work_root> <extras_root|''> <name> [dist_dir]
#   extras_root 传 '' 时只收 work 树；dist_dir 可选——其下的 *.txt
#   （smoke-gdb-* 取证文件等）收进 <name>/dist/。dist 在 work 根之外，
#   不传就丢 / pass '' for extras_root to collect only the work tree;
#   dist_dir is optional — *.txt under it (smoke-gdb-* forensics etc.)
#   lands in <name>/dist/. dist lives outside the work root; without
#   the arg those files are lost
set -u

logs="$1"
work="$2"
extra="${3:-}"
name="$4"
dist="${5:-}"
mkdir -p "$logs"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# ── file-tree.txt：两个根的全量清单（行与包内路径一致；.git 内部对
# 诊断无用且动辄数万行，剪去）──
# file-tree.txt: the full manifest of both roots (lines match in-bundle
# paths; .git internals are diagnostic dead weight and thousands of
# lines long — pruned)
{
  [ -d "$work" ] && (cd "$work" \
    && find . \( -name .git -type d -o -path './src-cache/*/src' \) -prune \
      -o -type f -print | sed "s|^\./|$name/|")
  [ -n "$extra" ] && [ -d "$extra" ] \
    && (cd "$extra" \
      && find . -name .git -type d -prune -o -type f -print | sed "s|^\./|$name/.extras/|")
  true
} | sort > "$logs/file-tree.txt" || true

# ── work 树：*.txt / *.log / failed 失败现场整树 → logs/<name>/ ──
# （src-cache 的源码检出同样只有噪声：.git 与解包源树的杂散 txt 都剪去，
# 需要的现场由 failed/ 抢救目录承担）
# work tree: *.txt / *.log / the whole failed/ scene -> logs/<name>/
# (the src-cache checkouts are noise too — .git plus stray txts of
# unpacked source trees are pruned; the failed/ salvage dir carries the
# scene that matters)
if [ -d "$work" ]; then
  (cd "$work" \
    && find . \( -name .git -type d -o -name src-cache -type d \) -prune \
      -o -type f \( -name '*.txt' -o -name '*.log' -o -path './failed/*' \) -print \
    | sort > "$scratch/wlist") || true
  mkdir -p "$logs/$name"
  # tar 双向 -C 保持相对结构，比逐文件 cp 快 / two-sided tar -C keeps the
  # relative structure and beats per-file cp
  [ -s "$scratch/wlist" ] \
    && (cd "$work" && tar -cf - -T "$scratch/wlist") \
    | (cd "$logs/$name" && tar -xf -) || true
fi

# ── dist 的 *.txt（smoke-gdb-* 取证文件等）→ logs/<name>/dist/ ──
# dist 在 work 根之外：归档走自己的上传 glob，这里只收诊断向 txt。
# smoke 挂在打包前——build-* 产物包因无归档被整个跳过，这文件只有
# 本包能见到 / dist *.txt (smoke-gdb-* forensics etc.) ->
# logs/<name>/dist/. dist sits outside the work root: archives ship
# via their own glob; only diagnostic txts join this bundle. A smoke
# failure precedes packaging — the build-* artifact is skipped for
# lack of archives, so this bundle is the file's only window out
if [ -n "$dist" ] && [ -d "$dist" ]; then
  (cd "$dist" && find . -type f -name '*.txt' | sort > "$scratch/dlist") || true
  mkdir -p "$logs/$name/dist"
  [ -s "$scratch/dlist" ] \
    && (cd "$dist" && tar -cf - -T "$scratch/dlist") \
    | (cd "$logs/$name/dist" && tar -xf -) || true
fi

# ── extras 状态目录全量 → logs/<name>/.extras/ ──
# whole extras state dir -> logs/<name>/.extras/
if [ -n "$extra" ] && [ -d "$extra" ]; then
  (cd "$extra" && find . -type f | sort > "$scratch/elist") || true
  mkdir -p "$logs/$name/.extras"
  [ -s "$scratch/elist" ] \
    && (cd "$extra" && tar -cf - -T "$scratch/elist") \
    | (cd "$logs/$name/.extras" && tar -xf -) || true
fi

# ── errors.txt：本包出错索引——必要清单之外先给"哪里错了"指路 ──
# errors.txt: the failure index — beyond the plain manifests it points
# straight at what went wrong
{
  echo "# 本日志包出错索引 / failure index of this log bundle"
  echo

  echo "## extras 配方失败 / failed extras recipes"
  fails="$(find "$logs" \( -name 'status.txt' -o -name 'extras-status.txt' \) \
    -exec grep -h ' failed$' {} + 2>/dev/null | awk '{print $1}' | sort -u)"
  if [ -n "$fails" ]; then
    while IFS= read -r n; do
      [ -z "$n" ] && continue
      l="$(find "$logs" -name "$n.log" | head -n1)"
      echo "- $n${l:+  (log: ${l#"$logs"/})}"
    done <<<"$fails"
  else
    echo "(无记录 / none recorded)"
  fi
  echo

  echo "## FFmpeg configure 丢包 / configure-dropped libraries"
  # dropped-libs{variant}.txt 逐变体枚举——同干 config-log 按
  # 变体 stem 配对 / enumerate every variant's dropped-libs{variant}
  # .txt — sibling config-logs pair by the variant stem
  any=0
  while IFS= read -r d; do
    [ -s "$d" ] || continue
    any=1
    stem="$(basename "$d" .txt)"
    echo "### $stem"
    while IFS= read -r line; do
      [ -z "$line" ] && continue
      n="${line%% *}"
      cl="$(find "$logs" -name "$stem-$n.config-log.txt" | head -n1)"
      echo "- $line${cl:+  (config.log: ${cl#"$logs"/})}"
    done < "$d"
  done < <(find "$logs" -name 'dropped-libs*.txt' ! -name '*.fflog.txt' ! -name '*.config-log.txt' | sort)
  [ "$any" -eq 0 ] && echo "(无 / none)"
  echo

  echo "## 本包详细日志 / detailed logs in this bundle"
  find "$logs" -type f \( -name '*.log' -o -name '*log*.txt' -o -path '*/failed/*' \) \
    | sed "s|^$logs/|- |" | sort
} > "$logs/errors.txt" || true
