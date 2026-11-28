# sources 全量钉版化实施计划 / sources full-pinning implementation plan

> 2026-11-28 用户指令原始需求 / original user directive of 2026-11-28：
> 「目前是构建时实时获取最新结果的，容易出问题或者达到限流，现在改成
> 全固定的，要求不变，现在的实时获取最新结果改成更新 sources 中 yaml
> 文件的单独程序，这个程序也要 CI 但是不用 Release」

## 需求分解 / requirement breakdown

1. **构建期零浮动**：`sources/*.yaml` 全部固化（每包精确 commit hash），
   构建驱动只做 fetch+checkout，不再做版本解析、tag 筛选、GitLab CI
   门查询——网络面缩到纯克隆 / zero floating at build time: every
   package pinned to an exact commit; the driver only fetches+checks out —
   no version resolution, tag filtering, or GitLab CI gate queries
2. **解析引擎不死**：现有实时解析逻辑（latest_tag/is_version_tag/
   gitlab CI 门/rolling master 追踪）抽离成**独立程序**（同仓新
   bin target 或子 crate），职责 = 重算每包最新合格 ref 并**回写
   sources yaml** / the resolution engine lives on as a standalone
   program (new bin target or sub-crate) whose sole job is recomputing
   each package's newest qualifying ref and rewriting the yamls
3. **更新器进 CI 不进 Release**：新工作流或 ci.yml 加 job
   （fmt/clippy/test/build），release.yml 产物矩阵不含它 / the
   updater gets CI (fmt/clippy/test/build) but no release artifacts
4. **要求不变**：解析语义（CI 绿门、版本样式 tag 优先、化石 tag 排除
   等）原样保留，只是执行时机从构建期挪到更新期 / resolution
   semantics unchanged — CI-green gates, version-looking tag priority,
   fossil exclusion — only the execution timing moves from build time
   to update time

## 架构设计 / architecture

### yaml schema（向后兼容式扩展 / backward-compatible extension)

```yaml
shine:
  git: https://github.com/toots/shine.git
  ref: abc123...        # 已有语义：显式钉 / existing: explicit pin
  pin: f4e2a1...40hex   # 新增：更新器回写的解析结果 / new: updater-written resolved commit
```

- `ref` 保留人工钉的最高优先；`pin` 是更新器产物；两者皆无=报错
  （不再有"浮动"态）/ `ref` stays the manual override; `pin` is the
  updater's output; neither present = hard error (no floating state left)
- 更新器回写时保留注释——yaml round-trip 需注释保持型解析（手绘
  emitter 或 yaml-edit 库）/ comment preservation on rewrite needs a
  round-trip emitter

### 构建驱动改造 / driver changes

- `src/core/net.rs` / `src/manifest/` 解析层：删掉运行时 ls-remote/
  tag 筛选/CI 门路径，只读 ref/pin（保留浅克隆+filter=blob 优化）
- gitlab CI 门逻辑整体迁入更新器 / the gitlab CI gate moves wholesale
  into the updater

### 更新器程序 / the updater

- `src/bin/update-sources.rs`（或 `updater/` 子 crate）：复用 net.rs
  抽出的解析函数库化部分
- 输出：重写后的 yaml + 人读 diff 摘要（哪些包动了、commit 前进了
  多少、谁被 CI 门挡下）/ emits rewritten yamls + a human diff
  summary (what moved, how far, who got gate-blocked)
- 运行方式：手动 / workflow_dispatch 定时——产物是 PR 或直接推
  master（待定）/ triggered manually or by schedule; output lands as
  a PR or direct push (TBD)

### CI / CI

- ci.yml 加 `update-sources` 的 fmt/clippy/test/build job（或独立
  updater.yml）；release.yml 矩阵明确不含 / CI gains updater jobs;
  the release matrix explicitly excludes it

## 施工顺序 / execution order

1. net.rs 解析逻辑库化（pub 函数抽出，行为零变）/ librarify the
   resolution logic with zero behavior change
2. yaml schema + 驱动只读 pin/ref（浮动路径删除）/ schema + driver
   reads pins only
3. update-sources 二进制（最薄壳：遍历 yaml → 解析 → 回写）
4. 全量包首轮 pin 回填（跑一遍更新器本身！）/ first full pin
   backfill — by running the updater itself
5. CI 工作流接线 / CI wiring
6. README 双语同步 / bilingual README sync

## 雷区备忘 / landmine memos

- yaml 注释丢失是最大易碎点——先验证 round-trip / comment loss is
  the top fragility — verify round-tripping first
- freetype 化石修复（is_version_tag 横杠版号）必须随迁更新器 / the
  freetype fossil fix must migrate with the updater
- legacy 腿用老系统库镜像，`pin` 解析要注意镜像内 git 版本下限 /
  legacy legs run old distro images — mind git version floors
- 回填首轮建议分批（十包一组跑 CI）防大爆炸 / backfill in batches
  to avoid big-bang failure

---

## 实施收官实录 / implementation completion record（2026-11-28）

六步施工全部落地 / all six execution steps landed:

1. ✅ 库化（353f6bd）2. ✅ schema+驱动只读化（00d83ef）
3. ✅ update-sources 二进制（ad1e3e1）4. ✅ 首轮回填（e4ce826）
5. ✅ CI 接线（4e36ca1）6. ✅ README 双语（2396f2a）

### 计划外实证修正 / empirical corrections beyond the plan

首轮回填暴露三处计划未预见的化石语义漏检，全部由真实解析输出
驱动修正（0d635ff）/ the first backfill exposed three fossil-semantics
leaks the plan did not foresee — all fixed driven by real resolution
output:

- **管家标记 tag** / housekeeping marker tags：openssl 回退抓
  `4.0-POST-CLANG-FORMAT-WEBKIT`——vkey 对数字后字母垃圾隐形。
  新增发布形结构判据（剥前导非数字缀后须纯数字链+可选预发布缀），
  ww 系处置从降级兜底升级为整体出局 / openssl's fallback fetched
  `4.0-POST-CLANG-FORMAT-WEBKIT` — vkey is blind to post-numeric
  alpha junk. A release-shape structural test was added; ww-style
  labels upgraded from demoted-fallback to full exclusion
- **裸版号优先层反噬** / bare-version preference backfired：libmfx
  远古裸版 1.x 压过嵌入式真发布系 intel-mediasdk-18/19/2x——嵌入式
  名称恰是仓主发布系本身。该层拆除，纯 vkey 最大者当选 / MediaSDK's
  ancient bare 1.x outranked the embedded intel-mediasdk release line —
  embedded names being the maintainer's own series. The layer was
  dismantled; pure vkey max wins
- **点式 pre 缀漏检** / dotted pre-suffix undetected：
  `intel-mediasdk-18.3.pre1` [18,3,1] 压过正式版 [18,3,0] / the
  `.pre` form outranked its own final

### 验证结果 / verification results

- lint 126 git 条目零违规（114 pin + 12 冻结 sha-ref 账平）/ lint
  clean across 126 git entries (114 pins + 12 frozen sha-refs)
- 跨宿主探针矩阵全过：git.ffmpeg.org(git://)/github/gitlab.com/
  videolan/freedesktop/xiph/gnome/bitbucket/googlesource 按 sha 裸
  fetch+checkout 均可物化 / the cross-host probe matrix passes: every
  host materializes pins by bare sha fetch + checkout
- 稳态复跑：110 未变 / 4 前进（活头移动）/ 0 失败 / steady-state
  re-run: 110 unchanged, 4 advanced (live heads moved), 0 failed
