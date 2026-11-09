//! extras 引擎：配方表驱动——跳过不适用平台的项，按
//! provided/cached/需构建三分支处理，跑配方、收尾 postfix、落状态文件。
//! Extras engine: drive the recipe table — skip entries that don't apply to
//! this platform, classify each as provided/cached/to-build, run recipes,
//! postfix, write status files.
//! 两处引擎约定 / Two engine conventions:
//! * FFmpeg 旗标不由配方收集——features/*.json 是旗标真源，配方表里的
//!   flags 字段只作归属记录（仍写 flags.txt 备查）
//!   FFmpeg flags are not collected from recipes — features/*.json is the
//!   flag source of truth; the table's flags field is provenance only
//!   (flags.txt still written for the record)
//! * 不跑补丁环节——先保持干净源码构建
//!   no patch phase — clean upstream sources only for now

use crate::fsx;
use crate::log;
use crate::pkgcfg;
use crate::sys;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 配方表项 / recipe table entry
pub struct Recipe {
    /// 配方名（.ok 标记、状态行、工作目录共用）
    /// recipe name (shared by the .ok marker, status line and work dir)
    pub name: &'static str,
    /// 适用目标："all" | "<os>" | "<os>:<arch>"，逗号分隔
    /// targets: "all" | "<os>" | "<os>:<arch>", comma separated
    pub targets: &'static str,
    /// 构建后验收规格（check_token 集合）
    /// post-build acceptance spec (check_token set)
    pub check: &'static str,
    /// 归属的 FFmpeg 特性名（仅记录；旗标由 features.json 解析）
    /// owning FFmpeg feature name (record only; features.json resolves flags)
    pub flags: &'static str,
    /// 配方体 / recipe body
    pub run: fn(&Ctx) -> Result<(), String>,
    /// 配方所在分组文件源码（.ok 哈希输入；include_str! 注入）
    /// source of the group file the recipe lives in (.ok hash input,
    /// injected via include_str!)
    pub src: &'static str,
}

/// 配方执行上下文 / recipe execution context
pub struct Ctx {
    /// 本配方的干净工作目录 / this recipe's clean working dir
    pub wd: PathBuf,
    /// 配方名（sources 清单默认键）/ recipe name (default sources key)
    pub name: &'static str,
}

impl Ctx {
    /// 本配方的源码：sources/<os>-<arch>.yaml+common.yaml 清单解析
    /// （git 实时+CI 回退 / 固定归档），优先取预取缓存
    /// This recipe's own source: resolved from the sources/*.yaml map
    /// (git realtime+CI fallback / fixed archive), preferring the
    /// prefetch cache
    pub fn src(&self) -> Result<PathBuf, String> {
        crate::sources::materialize(&self.wd, self.name)
    }

    /// 取清单里另一个库的源码（多源配方——vulkan/lilv 栈）
    /// Materialise another library from the map (multi-source recipes —
    /// the vulkan/lilv stacks)
    pub fn src_named(&self, name: &str) -> Result<PathBuf, String> {
        crate::sources::materialize(&self.wd, name)
    }

    /// fetch_tar 到 wd/src 并返回 src 路径 / fetch_tar into wd/src
    pub fn tar(&self, url: &str, sha: Option<&str>) -> Result<PathBuf, String> {
        crate::net::fetch_tar(&self.wd, url, sha)
    }

    /// fetch_git 到 wd/src 并返回 src 路径 / fetch_git into wd/src
    pub fn git(&self, url: &str, refname: &str, sha: Option<&str>) -> Result<PathBuf, String> {
        crate::net::fetch_git(&self.wd, url, refname, sha)
    }

    /// fetch_git_commit 到 wd/src 并返回 src 路径 / fetch_git_commit into wd/src
    pub fn git_commit(&self, url: &str, sha: &str) -> Result<PathBuf, String> {
        crate::net::fetch_git_commit(&self.wd, url, sha)
    }

    /// 时间戳文件：vendored 清单的起点 / stamp file for vendored manifests
    pub fn stamp(&self) -> PathBuf {
        let p = self.wd.join(".vend-stamp");
        fsx::write(&p, "").ok();
        p
    }
}

/// extras 状态目录 <prefix>/.extras / state dir <prefix>/.extras
pub fn state_dir() -> PathBuf {
    sys::env().prefix_win.join(".extras")
}

/// extras 工作目录 <work>/extras / scratch dir <work>/extras
pub fn work_dir() -> PathBuf {
    sys::env().work.join("extras")
}

/// extras_applies：配方是否适用于当前平台
/// Whether the recipe applies to this platform
pub fn applies(targets: &str) -> bool {
    let e = sys::env();
    targets.split(',').any(|tok| {
        let mut it = tok.trim().splitn(2, ':');
        let os = it.next().unwrap_or("");
        let arch = it.next();
        (os == "all" || os == e.os.name()) && (arch.is_none() || arch == Some(e.arch.name()))
    })
}

/// extras_provided：平台自带栈已把库装进前缀（pc 模块必须来自平台栈
/// 自己的前缀、静态归档门槛也要过）/ the platform's own stack already
/// installed the library (pc modules must come from the platform stack's
/// own prefix and the static-archive gate must pass)
pub fn provided(check: &str) -> bool {
    let e = sys::env();
    for c in check.split(',') {
        let c = c.trim();
        if c == "-" || c.starts_with("file:") || c.starts_with("hdrsym:") || c.starts_with("arsym:")
        {
            if pkgcfg::check_token(c).is_err() {
                return false;
            }
            continue;
        }
        if !pkgcfg::exists(c) {
            return false;
        }
        // .pc 必须来自 $MSYSTEM_PREFIX（pacman 这类平台栈自己的树）且
        // 不属于 deps 前缀——系统 -dev 包不算"已提供"；deps 前缀里无
        // .ok 的 .pc 更不算：那是快照残留（配方建过或该建的），骑平台
        // 捷径会永久跳过重建。实证：fontconfig 配方上线时快照已带
        // 2.16 文件且无 .ok，provided 永远放行 → pango 卡 >=2.17。
        // linux/mac 上 env_prefix==prefix，此条件恒假——跳过权全部
        // 归还给 .ok 哈希校验 / the .pc must come from $MSYSTEM_PREFIX
        // (the platform stack's own tree, e.g. pacman) yet NOT the deps
        // prefix — a system -dev package is not "provided", and neither
        // is an .ok-less .pc inside the deps prefix: that is snapshot
        // residue from a recipe that built or should build it, and
        // riding the platform shortcut skips the rebuild forever.
        // Empirical: the fontconfig recipe went live while snapshots
        // already carried 2.16 files and no .ok — provided let it
        // through permanently and pango's >=2.17 check failed.
        // On linux/mac env_prefix==prefix so this is always false —
        // skip authority returns to the .ok hash check entirely
        let pcd = pkgcfg::variable(c, "pcfiledir");
        let from_platform = !e.env_prefix.is_empty()
            && pcd.starts_with(e.env_prefix.as_str())
            && (e.prefix.is_empty() || !pcd.starts_with(e.prefix.as_str()));
        if !from_platform {
            return false;
        }
    }
    pkgcfg::static_libs_ok(check).is_ok()
}

/// 配方 .ok 标记的期望哈希：配方分组源码 + 工具链指纹 + 同名源规格。
/// 源规格（git ref / archive sha256 / submodules）必须参与：钉版本一变，
/// 恢复出来的 .ok 就得失效，否则旧版本快照无限续命（实证：fontconfig
/// 已钉 2.17+，2.16 的 provided 仍放行，pango 卡 >=2.17 全平台挂）。
/// 同名键缺席按空处理（异名源配方如 ffnvcodec→nv-codec-headers）；
/// 多源配方只覆盖主源——次要源换版时一般也伴随配方改动。
/// Expected .ok hash: group source + toolchain fingerprint + same-named
/// source spec. The spec (git ref / archive sha256 / submodules) must
/// take part: a bumped version pin has to invalidate a restored .ok, or
/// the stale snapshot rides forever (empirical: fontconfig stayed
/// "provided" at 2.16 after the 2.17+ pin and pango's >=2.17 check
/// failed on every platform). A missing same-named key counts as empty
/// (renamed-source recipes like ffnvcodec -> nv-codec-headers);
/// multi-source recipes cover the primary source only — a secondary
/// source bump normally rides a recipe change anyway.
pub fn recipe_sum(r: &Recipe) -> Result<String, String> {
    let tc = crate::deps::toolchain();
    let spec = crate::sources::spec(r.name)
        .map(|s| format!("{s:?}"))
        .unwrap_or_default();
    sys::sha256_data(format!("{}{tc}{spec}", r.src).as_bytes())
}

/// extras_run：主循环 / main loop
pub fn run_all() -> Result<(), String> {
    let e = sys::env();
    let state = state_dir();
    let work = work_dir();
    fsx::install_d(&state)?;
    fsx::install_d(&work)?;
    fsx::write(&state.join("flags.txt"), "")?;
    fsx::write(&state.join("status.txt"), "")?;
    // "done" 只在整表走完后才触碰——半途死掉的 status.txt 绝不能被
    // --save-deps 误当成一次完成的 extras 扫描
    // "done" is only touched once the whole table has been visited — a
    // status.txt left by a mid-pass death must not be mistaken for a
    // finished extras pass by --save-deps
    let _ = std::fs::remove_file(state.join("done"));

    // 直调 autoreconf 的配方需要 aclocal 看到装进前缀的宏——在本进程环境
    // 导出一次，所有子进程继承 / recipes calling autoreconf directly need
    // aclocal to see prefix-installed macros — export once here so every
    // child inherits it
    let aclocal = std::env::var("ACLOCAL_PATH").unwrap_or_default();
    // SAFETY：引擎循环开始、尚无并发任务 / SAFETY: engine loop start, no
    // concurrent tasks yet
    unsafe {
        std::env::set_var(
            "ACLOCAL_PATH",
            if aclocal.is_empty() {
                format!("{}/share/aclocal", e.prefix)
            } else {
                format!("{}/share/aclocal:{aclocal}", e.prefix)
            },
        );
        if e.os != sys::Os::Windows {
            // 前缀不在编译器默认搜索路径上——跨配方探测（nettle 找
            // -lgmp、libpng 找 zlib.h 这类不经 pkg-config 的原始检查）
            // 需要显式 -I/-L。Windows 的 msys env_prefix 已在编译器
            // 路径上，无需重复 / the prefix is not on the compiler's
            // default search path — cross-recipe probes that bypass
            // pkg-config (nettle's raw -lgmp check, libpng looking for
            // zlib.h) need explicit -I/-L. The msys env_prefix already
            // sits on the compiler path on Windows
            let inc = format!("-I{}/include", e.prefix);
            let lib = format!("-L{}/lib", e.prefix);
            for var in ["CPPFLAGS", "CFLAGS", "CXXFLAGS"] {
                let cur = std::env::var(var).unwrap_or_default();
                if !cur.contains(&inc) {
                    std::env::set_var(var, format!("{cur} {inc}").trim());
                }
            }
            let cur = std::env::var("LDFLAGS").unwrap_or_default();
            if !cur.contains(&lib) {
                std::env::set_var("LDFLAGS", format!("{cur} {lib}").trim());
            }
        }
    }

    // 先修好重复的 .pc 键行——恢复的前缀可能带着重复 Libs.private 的
    // .pc，它会死在 extras_provided 的 pkg-config --exists 上
    // heal duplicated .pc keyword lines first — a restored prefix can carry
    // a repeated Libs.private that dies inside extras_provided's
    // `pkg-config --exists`
    pkgcfg::dedup_fields();

    let status = state.join("status.txt");
    let flagsf = state.join("flags.txt");
    // (配方名, 失败原因)——原因要进末尾的失败明细与 step summary，
    // status.txt 仍只写 "name failed" 两词（下游按词尾判定）/ (name,
    // reason) — the reason feeds the closing failure detail and the step
    // summary; status.txt keeps the bare "name failed" pair (downstream
    // code matches the line suffix)
    let mut failed: Vec<(String, String)> = Vec::new();

    let table = crate::recipes::table();
    // 预取全部适用配方的源码——下载/CI 查询在后台并发，配方编译在
    // 前台串行；多源配方的附加源名一并登记（本平台清单里没有的键会被
    // 静默跳过）/ prefetch every applicable recipe's source — downloads
    // and CI queries run concurrently in the background while recipes
    // compile serially in the foreground; the extra source names of
    // multi-source recipes are registered too (keys absent from this
    // platform's map are skipped quietly)
    let names: Vec<String> = table
        .iter()
        .filter(|r| applies(r.targets))
        .map(|r| r.name.to_string())
        .chain(
            [
                "vulkan-headers",
                "vulkan-shim",
                "opencl-headers",
                "opencl-icd",
                "spirv-cross",
                "lv2",
                "zix",
                "serd",
                "sord",
                "sratom",
                "lilv",
                // ffnvcodec 配方的源键与配方名不同 /
                // the ffnvcodec recipe's sources key differs from its name
                "nv-codec-headers",
            ]
            .iter()
            .map(|s| s.to_string()),
        )
        .collect();
    crate::sources::prefetch(names);

    for r in table {
        if !applies(r.targets) {
            continue;
        }
        let okf = state.join(format!("{}.ok", r.name));
        let rsum = recipe_sum(&r)?;

        // 已在平台自带栈里进了前缀：只记旗标，不再构建；但有 .ok 标记且
        // 配方已改（rsum 不同）时不能骑 provided 捷径继续用过期归档
        // already in the prefix from the platform's own stack: record flags
        // only; but a stale .ok with a changed recipe must not ride the
        // provided shortcut
        if provided(r.check) && (!okf.is_file() || fsx::read(&okf).trim() == rsum) {
            if r.flags != "-" {
                fsx::append_line(&flagsf, r.flags)?;
            }
            fsx::append_line(&status, &format!("{} provided", r.name))?;
            log::log(&format!("[extras] {}: provided by platform", r.name));
            continue;
        }
        if okf.is_file()
            && fsx::read(&okf).trim() == rsum
            && pkgcfg::check_spec(r.check).is_ok()
            && pkgcfg::static_libs_ok(r.check).is_ok()
        {
            if r.flags != "-" {
                fsx::append_line(&flagsf, r.flags)?;
            }
            fsx::append_line(&status, &format!("{} cached", r.name))?;
            log::log(&format!("[extras] {}: cached", r.name));
            continue;
        }

        log::group_begin(&format!("extras: {}", r.name));
        let wd = work.join(r.name);
        fsx::rm_rf(&wd);
        fsx::install_d(&wd)?;
        let ctx = Ctx {
            wd: wd.clone(),
            name: r.name,
        };

        // 配方体是进程内 Rust 函数，但为了给每配方落一份完整日志，用
        // `recipe-run` 子命令把它作为子进程重入：输出仍直通本进程日志
        // （CI 折叠组提供隔离），run_tee 同步写 <work>/extras/<name>.log
        // ——失败时日志包里就有完整现场，而不是只剩 status.txt 一行
        // "failed"
        // the recipe body is an in-process Rust fn, but to land a complete
        // per-recipe log it is re-entered as a subprocess via the
        // `recipe-run` subcommand: output still streams to our log (CI fold
        // groups isolate it) while run_tee mirrors it into
        // <work>/extras/<name>.log — a failure leaves the whole scene in the
        // log bundle instead of a lone "failed" line in status.txt
        // 预取句柄在父进程——派生前先等本配方的源码落地 done 标记，
        // 让子进程的 materialize 走磁盘缓存命中而不是重复下载
        // the prefetch handle lives in the parent — await this recipe's
        // done marker before spawning so the child's materialize hits
        // the on-disk cache instead of downloading twice
        crate::sources::wait_prefetch(r.name);
        let rlog = work.join(format!("{}.log", r.name));
        let rc = match std::env::current_exe() {
            Ok(exe) => {
                match sys::run_tee(&exe.to_string_lossy(), &["recipe-run", r.name], &wd, &rlog) {
                    Ok(code) => Ok(code),
                    Err(spawn_err) => {
                        // 派生失败退回进程内执行——日志少了但配方不丢 / spawn
                        // failure falls back to the in-process call — the log
                        // is lost but the recipe still runs
                        log::warn(&format!(
                            "recipe-run spawn failed ({spawn_err}); running in-process"
                        ));
                        (r.run)(&ctx).map(|_| 0)
                    }
                }
            }
            Err(spawn_err) => {
                log::warn(&format!(
                    "current_exe failed ({spawn_err}); running in-process"
                ));
                (r.run)(&ctx).map(|_| 0)
            }
        };

        // 验收错误先拎出来——它才是"为什么算失败"的答案，后面得补进
        // .log / lift the acceptance error out first — it is the answer
        // to "why did this count as failed" and must reach the .log
        let check_err = if rc == Ok(0) {
            pkgcfg::check_spec(r.check)
                .err()
                .or_else(|| pkgcfg::static_libs_ok(r.check).err())
        } else {
            None
        };

        if rc == Ok(0) && check_err.is_none() {
            fsx::write(&okf, &format!("{rsum}\n"))?;
            if r.flags != "-" {
                fsx::append_line(&flagsf, r.flags)?;
            }
            fsx::append_line(&status, &format!("{} ok", r.name))?;
            log::log(&format!("[extras] {}: ok", r.name));
        } else {
            let reason = match &rc {
                Err(e) => format!("recipe error: {e}"),
                Ok(0) => format!(
                    "post-build acceptance check failed: {}",
                    check_err.as_deref().unwrap_or("unknown")
                ),
                // 非零退出本身不说明任何事——把 .log 的错误尾行带进原因，
                // CI 上点开折叠组之前就能看见具体错 / a bare exit code
                // explains nothing — carry the log's error tail in the
                // reason so the concrete failure is visible without
                // opening the fold group
                Ok(code) => {
                    let tail = error_tail(&rlog, 6);
                    if tail.is_empty() {
                        format!("recipe error: recipe-run exited {code}")
                    } else {
                        format!("recipe-run exited {code}: {tail}")
                    }
                }
            };
            println!("{reason}");
            // CI 下 run_tee 不回显子进程输出——单行摘要之外再把失败配方
            // 日志的 verbatim 尾部打出来，控制台里看得见现场；全量仍在
            // <name>.log / run_tee does not echo child output under CI —
            // beyond the one-line digest print the failed recipe's
            // verbatim log tail so the console shows the scene; the full
            // log still lives in <name>.log
            println!("{}", sys::log_tail(&rlog, 40));
            // 验收失败时 .log 里只有一段干净的构建输出——把失败结论也补进
            // 去，否则日志包看得见成功、看不见为什么算失败 / on an
            // acceptance failure the .log holds only a clean build —
            // append the verdict so the bundle shows why it counts as
            // failed
            let _ = fsx::append_line(&rlog, &format!("recipe-run {}: {reason}", r.name));
            fsx::append_line(&status, &format!("{} failed", r.name))?;
            log::warn(&format!(
                "optional library '{}' failed to build and is left out of this build",
                r.name
            ));
            failed.push((r.name.to_string(), reason.clone()));
            // 失败现场在 wd 删除前抢救：<name>.log 只是输出流，
            // config.log 这类文件才是"检查为何没过"的答案
            // salvage the scene before wd is deleted: <name>.log is the
            // output stream; files like config.log answer why the checks
            // failed
            salvage_failure(&wd, r.name);
        }
        log::group_end();
        fsx::rm_rf(&wd);
    }

    // postfix（.pc 元数据修补+归档去重）与配方同权：group + run_tee 落
    // extras/postfix.log——它动的是全配方共享的 .pc/归档，出问题光看单
    // 配方日志根本定位不到 / postfix (.pc-metadata patch + archive dedup)
    // gets recipe-grade treatment: its own group + run_tee into
    // extras/postfix.log — it mutates .pc files/archives shared by every
    // recipe, and a breakage is undiagnosable from any single recipe log
    log::group_begin("extras: postfix (patch pkg-config metadata)");
    let plog = work.join("postfix.log");
    let prc = match std::env::current_exe() {
        Ok(exe) => match sys::run_tee(&exe.to_string_lossy(), &["postfix-run"], &work, &plog) {
            Ok(code) => Ok(code),
            Err(spawn_err) => {
                log::warn(&format!(
                    "postfix-run spawn failed ({spawn_err}); running in-process"
                ));
                postfix().map(|_| 0)
            }
        },
        Err(spawn_err) => {
            log::warn(&format!(
                "current_exe failed ({spawn_err}); running in-process"
            ));
            postfix().map(|_| 0)
        }
    };
    log::group_end();

    if prc != Ok(0) {
        // Ok(非零) 是子进程失败；Err 是进程内调用失败——两种口径归一成
        // 一句原因 / Ok(nonzero) = subprocess failure; Err = in-process
        // call failure — fold both into one reason
        let why = match &prc {
            Err(e) => e.clone(),
            Ok(code) => {
                let tail = error_tail(&plog, 6);
                if tail.is_empty() {
                    format!("postfix-run exited {code}")
                } else {
                    format!("postfix-run exited {code}: {tail}")
                }
            }
        };
        println!("archive post-processing failed: {why}");
        // 同上的 verbatim 尾部 / same verbatim tail as the recipe path
        println!("{}", sys::log_tail(&plog, 40));
        fsx::append_line(&status, "postfix failed")?;
        let _ = fsx::append_line(&plog, &format!("postfix-run: {why}"));
        let _ = std::fs::copy(&status, work.join("extras-status.txt"));
        let _ = std::fs::copy(&flagsf, work.join("extras-flags.txt"));
        step_summary_failures(e, &[("archive post-processing".to_string(), why.clone())]);
        return Err(format!(
            "archive post-processing failed, refusing to configure FFmpeg: {why}"
        ));
    }

    log::log("[extras] summary:");
    print!("{}", fsx::read(&status));
    let _ = std::fs::copy(&status, work.join("extras-status.txt"));
    let _ = std::fs::copy(&flagsf, work.join("extras-flags.txt"));

    // 失败的配方让它的库进不了最终二进制。上面的失败被收集而不中断，
    // 一次 run 能显示全部；这道门槛在 FFmpeg 还没配置时把它们变成致命
    // a failed recipe leaves its library out of the final binary. Failures
    // were collected without stopping so one run shows all of them; this
    // gate turns them fatal before FFmpeg is even configured
    if !failed.is_empty() {
        // 明细再打一次——summary 段是折叠组外最容易被看到的地方；同时写
        // $GITHUB_STEP_SUMMARY，run 首页直接给出具体错因 / print the
        // reasons once more under the summary — the spot most likely to
        // be seen outside fold groups — and into $GITHUB_STEP_SUMMARY so
        // the run page shows the concrete cause
        log::log("[extras] failure details:");
        for (n, why) in &failed {
            log::log(&format!("  {n}: {why}"));
        }
        step_summary_failures(e, &failed);
        return Err(format!(
            "required libraries failed to build, refusing to continue: {}",
            failed
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }

    // 全绿才算跑完——done 是 save-deps 的"完成快照"判定。此前它在失败
    // 检查之前无条件落盘，脏前缀被打上"完成"时间戳快照发出去 / only an
    // all-green pass earns done — save-deps reads it as the complete-
    // snapshot signal. It used to be written unconditionally before the
    // failure check, so dirty prefixes shipped as timestamped "complete"
    // snapshots
    fsx::write(&state.join("done"), "")?;
    Ok(())
}

/// 把失败明细追加到 $GITHUB_STEP_SUMMARY（CI run 首页）；本地运行/未设
/// 时静默跳过。reason 可能含 `|`，用列表不用表格 / Append the failure
/// detail to $GITHUB_STEP_SUMMARY (the CI run's front page); quietly skip
/// when unset (local runs). Reasons may contain `|` — a bullet list, not
/// a table
fn step_summary_failures(e: &sys::Env, failed: &[(String, String)]) {
    let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") else {
        return;
    };
    if path.is_empty() {
        return;
    }
    let mut md = format!(
        "## deps extras — {} — failed recipes / 失败配方\n\n",
        e.plat()
    );
    for (n, why) in failed {
        md.push_str(&format!("- **{n}** — {why}\n"));
    }
    md.push_str("\n详见日志工件中的 `<name>.log` / see `<name>.log` in the logs artifact\n");
    let _ = fsx::append_line(Path::new(&path), &md);
}

/// 从 <name>.log 抽"像错误"的尾行：非零退出的配方在日志里只留下整段
/// 构建输出，光报 "exited 1" 没人知道为什么。取最后几行匹配错误样式
/// 的内容（编译/链接/配置器的标准措辞），都没有就退化为末尾非空行。
/// Extract the error-looking tail lines from <name>.log: a recipe that
/// exited nonzero leaves a wall of build output, and a bare "exited 1"
/// says nothing. Take the last lines matching stock error phrasing from
/// compilers/linkers/configurers; fall back to the last non-empty lines.
fn error_tail(log: &Path, max_lines: usize) -> String {
    const PATTERNS: &[&str] = &[
        "error",
        "failed",
        "undefined",
        "not found",
        "no such",
        "cannot",
        "fatal",
        "missing",
        "ld returned",
        "ninja: build stopped",
    ];
    let text = fsx::read(log);
    let lines: Vec<&str> = text.lines().collect();
    // 小写化匹配——Error/ERROR/error 一族措辞全收 / case-folded matching
    // — catches the Error/ERROR/error family uniformly
    let mut hits: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            let t = l.trim();
            if t.is_empty() {
                return false;
            }
            let low = t.to_lowercase();
            PATTERNS.iter().any(|p| low.contains(p))
        })
        .collect();
    if hits.is_empty() {
        hits = lines
            .iter()
            .copied()
            .filter(|l| !l.trim().is_empty())
            .collect();
    }
    let tail: Vec<&str> = hits
        .iter()
        .rev()
        .take(max_lines)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    // 单行汇总、限长——要进 status 行外的失败明细与 step summary，不能
    // 变成又一面日志墙 / one-line digest, length-capped — this feeds the
    // failure detail and the step summary, it must not become another
    // log wall
    let joined = tail
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" | ");
    let mut s: String = joined.chars().take(400).collect();
    if joined.chars().count() > 400 {
        s.push('…');
    }
    s
}

/// `recipe-run` 子命令（内部）：按名执行单个配方。run_all 把每个配方
/// 作为子进程重入——输出经 run_tee 直通父进程日志并同步落
/// <name>.log；子进程 exit code 即配方成败。
/// The internal `recipe-run` subcommand: run one recipe by name. run_all
/// re-enters every recipe through it as a subprocess so run_tee can pass
/// the output through to the parent's log while landing it in
/// <name>.log; the child exit code is the recipe's verdict.
pub fn run_recipe(name: &str) -> Result<(), String> {
    let table = crate::recipes::table();
    let r = table
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| format!("unknown recipe '{name}'"))?;
    let wd = work_dir().join(r.name);
    fsx::install_d(&wd)?;
    let ctx = Ctx { wd, name: r.name };
    (r.run)(&ctx)
}

/// `postfix-run` 子命令入口：与 recipe-run 同款子进程重入，让 run_tee
/// 能把整个修补过程逐字落进 extras/postfix.log / entry point of the
/// `postfix-run` subcommand: same subprocess re-entry as recipe-run so
/// run_tee can land the whole fixup verbatim in extras/postfix.log
pub fn run_postfix() -> Result<(), String> {
    postfix()
}

/// 配方失败后、工作目录被删前，把构建系统留下的诊断日志（config.log /
/// meson-log.txt / CMake 的 Error/Output/ConfigureLog 等）抢救到
/// <work>/failed/<name>/——<name>.log 是输出流，这些文件才是"检查为
/// 何没过"的现场。
/// After a recipe fails — before its work dir is deleted — salvage the
/// diagnostic logs the build systems left behind (config.log /
/// meson-log.txt / CMake's Error/Output/ConfigureLog etc.) into
/// <work>/failed/<name>/: <name>.log is the output stream; these files
/// are the scene of why the checks failed.
fn salvage_failure(wd: &Path, name: &str) {
    let dst = sys::env().work.join("failed").join(name);
    for f in fsx::walk(wd) {
        let n = f
            .file_name()
            .map(|x| x.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let keep = n.ends_with(".log") || n == "meson-log.txt" || n == "cmakeconfigchangelog.yaml";
        if !keep {
            continue;
        }
        let Ok(rel) = f.strip_prefix(wd) else {
            continue;
        };
        let _ = fsx::install(&f, &dst.join(rel));
    }
}

/// extras_vendored_manifest <前缀相对路径> <stamp>：把 <stamp> 之后写进
/// 前缀的文件清单落成 vendored 清单（仅 windows——前缀与 pacman 共树）
/// Record every file written into the prefix after <stamp> as a vendored
/// manifest (windows only — the prefix shares a tree with pacman there)
pub fn vendored_manifest(rel: &str, stamp: SystemTime) -> Result<(), String> {
    let e = sys::env();
    if e.os != sys::Os::Windows {
        return Ok(());
    }
    let root = sys::posix_to_win(&e.env_prefix, &e.msys2_root);
    let list = fsx::find_newer(&root, stamp);
    fsx::write(&root.join(rel), &format!("{}\n", list.join("\n")))
}

/// 等长记号在归档成员里做字节级改名
/// Equal-length byte rename inside every archive member that carries the
/// token
///
/// 实现：自解析归档头按偏移逐成员定位、原地改写。CLI `ar` 只能按名
/// 寻址——rust 归档里 `.rcgu.o` 重名成员成堆，`ar p` 永远只拿第一
/// 份，其余重名成员的引用留在旧名（实证：librav1e 改名后
/// `__RNv…___rust_alloc` undefined——定义所在成员被改、重名成员的
/// 引用没动）/ implementation: parse the archive header ourselves and
/// patch members in place by offset. The `ar` CLI addresses members by
/// name only — rust archives pile up duplicate `.rcgu.o` names where
/// `ar p` always yields the first copy, leaving the other same-named
/// members' references on the old name (empirical: after the librav1e
/// rename `__RNv…___rust_alloc` went undefined — the defining member
/// was renamed while duplicate-named members' references were not)
pub(crate) fn byte_rename(
    lib: &Path,
    pairs: &[(&str, &str)],
    _dir: &Path,
) -> Result<usize, String> {
    let map: HashMap<&[u8], &[u8]> = pairs
        .iter()
        .map(|(f, t)| (f.as_bytes(), t.as_bytes()))
        .collect();
    let mut buf = std::fs::read(lib).map_err(|e| format!("read {lib:?}: {e}"))?;
    if buf.len() < 8 || &buf[..8] != b"!<arch>\n" {
        return Err(format!("{lib:?}: not a standard ar archive"));
    }
    // NUL 分词：符号名在 strtab 里以 \x00 分隔——切成 token 流逐词查表
    // 单趟 O(n)，且精确命中边界不会误伤子串；顺带自动跳过 `//` 长名表
    // (\n 分隔无 \x00 token) 与成员头(≤16B 名段装不下 mangled 名)
    // / NUL-tokenize: symbol names are \x00-delimited inside string
    // tables — split into tokens and map each in one O(n) pass with
    // exact-boundary hits that cannot bruise substrings; automatically
    // skips the `//` long-name table (\n-delimited, no \x00 tokens) and
    // member headers (the ≤16B name field can't hold mangled names)
    fn patch_tokens(data: &mut [u8], map: &HashMap<&[u8], &[u8]>) -> bool {
        let mut hit = false;
        let mut i = 0;
        while i < data.len() {
            if data[i] == 0 {
                i += 1;
                continue;
            }
            let start = i;
            while i < data.len() && data[i] != 0 {
                i += 1;
            }
            if let Some(&t) = map.get(&data[start..i]) {
                data[start..i].copy_from_slice(t);
                hit = true;
            }
        }
        hit
    }
    // 成员头数字段是右对齐空格填充的 ASCII 十进制 /
    // header numeric fields are space-padded ASCII decimals
    fn ascii_dec(b: &[u8]) -> usize {
        b.iter()
            .filter(|c| c.is_ascii_digit())
            .fold(0usize, |a, c| a * 10 + (c - b'0') as usize)
    }
    let mut renamed = 0usize;
    let mut pos = 8usize;
    while pos + 60 <= buf.len() {
        let hdr = &buf[pos..pos + 60];
        if &hdr[58..60] != b"`\n" {
            break;
        }
        let size = ascii_dec(&hdr[48..58]);
        let raw = &hdr[0..16];
        let mut dstart = pos + 60;
        // BSD `#1/N`：成员名前 N 字节内联在数据区开头 /
        // BSD `#1/N`: the member's first N bytes inline the name
        let inline = raw.strip_prefix(b"#1/").map(ascii_dec).unwrap_or(0);
        let name: &[u8] = if inline > 0 {
            buf.get(dstart..dstart + inline).unwrap_or(b"")
        } else {
            raw
        };
        // 索引/长名表成员跳过：GNU `/`、`//`、`/SYM64/`，BSD `__.SYMDEF*`
        // （含 #1/N 内联与直填两种名式）。`/NNN` 扩展名引用是普通成
        // 员，照常补丁 / skip index & long-name members: GNU `/`, `//`,
        // `/SYM64/`, BSD `__.SYMDEF*` (both #1/N-inline and direct-field
        // name forms). A `/NNN` extended-name reference is a normal
        // member — patch its data as usual
        let special = {
            let nz = name.iter().position(|c| *c == 0).unwrap_or(name.len());
            let mut end = nz;
            while end > 0 && name[end - 1] == b' ' {
                end -= 1;
            }
            let t = &name[..end];
            t == b"/"
                || t == b"//"
                || t == b"/SYM64/"
                || t == b"/SYM64"
                || t.starts_with(b"__.SYMDEF")
        };
        if inline > 0 {
            dstart += inline;
        }
        let dlen = size.saturating_sub(inline);
        if !special
            && dstart + dlen <= buf.len()
            && patch_tokens(&mut buf[dstart..dstart + dlen], &map)
        {
            renamed += 1;
        }
        pos += 60 + size + (size & 1);
    }
    std::fs::write(lib, &buf).map_err(|e| format!("write {lib:?}: {e}"))?;
    Ok(renamed)
}

/// 把"册内全局 UNDEF ∩ 局部定义"的 Mach-O 符号就地提升为外部符号
/// （`n_type |= N_EXT`），返回改动的 nlist 条目数 /
/// promote archive members' symbols that are "global UNDEF ∩ local def"
/// to external (`n_type |= N_EXT`) in place; returns patched nlist count
///
/// 起因：新版 rustc 把 `__rustc` 分配/panic shim 以局部符号（N_SECT
/// 无 N_EXT）发进静态库成员——同 crate-hash 的 librsvg 同类 shim 仍
/// 是全局（0x0f），两版 rustc 的 codegen 差异。局部定义够不着其它
/// 成员的外部 UNDEF，裸 `-lrav1e` 链测 `__rust_alloc`/`realloc`/
/// `start_panic`/`alloc_zeroed`/`panic_cleanup` 全家 undefined——
/// 与 dedup 无关，归档天生不自洽。提升判据"本册某成员对外引它 ∧
/// 本册某成员局部定义它"恰好命中 mis-localized 导出 /
/// cause: newer rustc emits the `__rustc` alloc/panic shim as LOCAL
/// symbols (N_SECT without N_EXT) inside staticlib members — the
/// same-crate-hash librsvg shim is still global (0x0f), a codegen
/// difference across rustc versions. A local definition can't satisfy
/// the other members' external UNDEFs, so the bare `-lrav1e` test dies
/// on the whole `__rust_alloc/realloc/start_panic/alloc_zeroed/
/// panic_cleanup` family — inherent self-inconsistency, unrelated to
/// dedup. The criterion "undef'd globally in ≥1 member ∧ defined
/// locally in ≥1 member" hits exactly the mis-localized exports
fn macho_export_shim_defs(lib: &Path) -> Result<usize, String> {
    let mut buf = std::fs::read(lib).map_err(|e| format!("read {lib:?}: {e}"))?;
    if buf.len() < 8 || &buf[..8] != b"!<arch>\n" {
        return Err(format!("{lib:?}: not a standard ar archive"));
    }
    fn ascii_dec(b: &[u8]) -> usize {
        b.iter()
            .filter(|c| c.is_ascii_digit())
            .fold(0usize, |a, c| a * 10 + (c - b'0') as usize)
    }
    let mut undefs: HashSet<Vec<u8>> = HashSet::new();
    let mut locals: Vec<(Vec<u8>, usize)> = Vec::new(); // (名, n_type 字节位置) / (name, file offset of n_type byte)
    let mut pos = 8usize;
    while pos + 60 <= buf.len() {
        let hdr = &buf[pos..pos + 60];
        if &hdr[58..60] != b"`\n" {
            break;
        }
        let size = ascii_dec(&hdr[48..58]);
        let inline = hdr[0..16].strip_prefix(b"#1/").map(ascii_dec).unwrap_or(0);
        let dstart = pos + 60 + inline;
        let dlen = size.saturating_sub(inline);
        pos += 60 + size + (size & 1);
        let Some(data) = buf.get(dstart..dstart + dlen) else {
            continue;
        };
        // 只认 64 位小端 Mach-O；`.SYMDEF`/位码/文本成员自然出局 /
        // 64-bit LE Mach-O only — SYMDEF/bitcode/text members bow out
        if data.len() < 32 || data[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
            continue;
        }
        let ncmds = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        let mut lc = 32usize;
        let (mut symoff, mut nsyms, mut stroff, mut strsize) = (0usize, 0usize, 0usize, 0usize);
        for _ in 0..ncmds {
            if lc + 8 > data.len() {
                break;
            }
            let cmd = u32::from_le_bytes(data[lc..lc + 4].try_into().unwrap());
            let csz = u32::from_le_bytes(data[lc + 4..lc + 8].try_into().unwrap()) as usize;
            if cmd == 0x2 && lc + 24 <= data.len() {
                // LC_SYMTAB / LC_SYMTAB
                symoff = u32::from_le_bytes(data[lc + 8..lc + 12].try_into().unwrap()) as usize;
                nsyms = u32::from_le_bytes(data[lc + 12..lc + 16].try_into().unwrap()) as usize;
                stroff = u32::from_le_bytes(data[lc + 16..lc + 20].try_into().unwrap()) as usize;
                strsize = u32::from_le_bytes(data[lc + 20..lc + 24].try_into().unwrap()) as usize;
            }
            lc = lc.saturating_add(csz.max(8));
        }
        if nsyms == 0 || symoff + nsyms * 16 > data.len() || stroff + strsize > data.len() {
            continue;
        }
        for i in 0..nsyms {
            let n = symoff + i * 16;
            let strx = u32::from_le_bytes(data[n..n + 4].try_into().unwrap()) as usize;
            let ntype = data[n + 4];
            if strx == 0 || strx >= strsize {
                continue;
            }
            let s = stroff + strx;
            let e = match data[s..stroff + strsize].iter().position(|c| *c == 0) {
                Some(k) => s + k,
                None => continue,
            };
            let name = data[s..e].to_vec();
            let ty = ntype & 0x0e;
            if ntype & 0xe0 != 0 || ntype & 0x10 != 0 {
                continue; // N_STAB / N_PEXT 不参与 / skip stab & private-extern
            }
            if ty == 0 && ntype & 0x01 != 0 {
                undefs.insert(name);
            } else if (ty == 0x0e || ty == 0x02) && ntype & 0x01 == 0 {
                locals.push((name, dstart + n + 4));
            }
        }
    }
    let mut flipped = 0usize;
    for (name, at) in locals {
        if undefs.contains(&name) {
            buf[at] |= 0x01; // 打 N_EXT / set N_EXT
            flipped += 1;
        }
    }
    if flipped > 0 {
        std::fs::write(lib, &buf).map_err(|e| format!("write {lib:?}: {e}"))?;
    }
    Ok(flipped)
}

/// 枚举 ar 归档全部成员 Mach-O 符号表里的外部定义名（N_EXT 置位 ∧
/// N_TYPE≠N_UNDF）。自解析替代 `nm -g`——实证 mac nm 在 4000+ 成员的
/// librsvg-2.a 上漏报：真实共享外部定义 933 个它只吐了 376，dedup 的
/// shared 集被腰斩，ffmpeg 终链 894 个 duplicate / enumerate external
/// definition names (N_EXT set ∧ N_TYPE≠N_UNDF) across every member's
/// Mach-O symtab in an ar archive. Self-parsing replaces `nm -g` —
/// empirically mac nm under-reports on the 4000+-member librsvg-2.a:
/// the true shared external-def set is 933 while it emitted only 376,
/// which halved dedup's `shared` set and left 894 duplicates at the
/// final ffmpeg link
fn macho_def_names(lib: &Path) -> HashSet<String> {
    let Ok(buf) = std::fs::read(lib) else {
        return HashSet::new();
    };
    if buf.len() < 8 || &buf[..8] != b"!<arch>\n" {
        return HashSet::new();
    }
    fn ascii_dec(b: &[u8]) -> usize {
        b.iter()
            .filter(|c| c.is_ascii_digit())
            .fold(0usize, |a, c| a * 10 + (c - b'0') as usize)
    }
    let mut out = HashSet::new();
    let mut pos = 8usize;
    while pos + 60 <= buf.len() {
        let hdr = &buf[pos..pos + 60];
        if &hdr[58..60] != b"`\n" {
            break;
        }
        let size = ascii_dec(&hdr[48..58]);
        let inline = hdr[0..16].strip_prefix(b"#1/").map(ascii_dec).unwrap_or(0);
        let dstart = pos + 60 + inline;
        let dlen = size.saturating_sub(inline);
        pos += 60 + size + (size & 1);
        let Some(data) = buf.get(dstart..dstart + dlen) else {
            continue;
        };
        // 只认 64 位小端 Mach-O / 64-bit LE Mach-O only
        if data.len() < 32 || data[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
            continue;
        }
        let ncmds = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        let mut lc = 32usize;
        let (mut symoff, mut nsyms, mut stroff, mut strsize) = (0usize, 0usize, 0usize, 0usize);
        for _ in 0..ncmds {
            if lc + 8 > data.len() {
                break;
            }
            let cmd = u32::from_le_bytes(data[lc..lc + 4].try_into().unwrap());
            let csz = u32::from_le_bytes(data[lc + 4..lc + 8].try_into().unwrap()) as usize;
            if cmd == 0x2 && lc + 24 <= data.len() {
                // LC_SYMTAB / LC_SYMTAB
                symoff = u32::from_le_bytes(data[lc + 8..lc + 12].try_into().unwrap()) as usize;
                nsyms = u32::from_le_bytes(data[lc + 12..lc + 16].try_into().unwrap()) as usize;
                stroff = u32::from_le_bytes(data[lc + 16..lc + 20].try_into().unwrap()) as usize;
                strsize = u32::from_le_bytes(data[lc + 20..lc + 24].try_into().unwrap()) as usize;
            }
            lc = lc.saturating_add(csz.max(8));
        }
        if nsyms == 0 || symoff + nsyms * 16 > data.len() || stroff + strsize > data.len() {
            continue;
        }
        for i in 0..nsyms {
            let n = symoff + i * 16;
            let strx = u32::from_le_bytes(data[n..n + 4].try_into().unwrap()) as usize;
            let ntype = data[n + 4];
            if strx == 0 || strx >= strsize {
                continue;
            }
            if ntype & 0xe0 != 0 || ntype & 0x10 != 0 {
                continue; // N_STAB / N_PEXT 不参与 / skip stab & private-extern
            }
            if ntype & 0x01 == 0 || ntype & 0x0e == 0 {
                continue; // 只要外部且已定义 / extern ∧ defined only
            }
            let s = stroff + strx;
            let e = match data[s..stroff + strsize].iter().position(|c| *c == 0) {
                Some(k) => s + k,
                None => continue,
            };
            out.insert(String::from_utf8_lossy(&data[s..e]).into_owned());
        }
    }
    out
}

/// nm 里是否有匹配模式行的符号（[TtWw]/_? 前缀）/ whether nm output has a
/// defined/weak symbol matching the name (with optional _ prefix)
fn nm_has(lib: &Path, sym: &str, dir: &Path) -> bool {
    crate::ar::has_sym(lib, sym, dir)
}

/// postfix 专属显迹 libs_private：目标 .pc 缺席时落一行跳过；token 已在
/// 文件里落一行已存在——postfix.log 是 .pc 修补的唯一现场，"没动这个
/// 文件、为什么没动"与"动了什么"一样要能查 / postfix-only traced
/// libs_private: log a skip line when the target .pc is absent and an
/// already-present line for carried tokens — postfix.log is the sole
/// scene of .pc surgery, so "untouched, and why" must be as auditable
/// as "touched"
fn libs_private_tr(name: &str, libs: &[&str]) {
    let f = pkgcfg::pc_file(name);
    if !f.is_file() {
        log::log(&format!(
            "[extras] postfix: {name}.pc absent — {} skipped",
            libs.join(" ")
        ));
        return;
    }
    let mut pending: Vec<&str> = Vec::new();
    for t in libs {
        if pkgcfg::token_present(&f, t) {
            log::log(&format!("[extras] postfix: {name}.pc already carries {t}"));
        } else {
            pending.push(*t);
        }
    }
    pkgcfg::libs_private(name, &pending);
}

/// extras_postfix：对已装归档和 .pc 文件做去重/修补——针对单个配方看不
/// 见的链接期问题 / dedup/fixup pass over the installed archives and pc
/// files for link-time problems no single recipe can see
fn postfix() -> Result<(), String> {
    let e = sys::env();
    let libdir = e.prefix_win.join("lib");
    let work = e.work.clone();
    // 起止两行必须无条件落——全部分支都跳过时这是 run_tee 落进
    // postfix.log 的唯一字节，空文件跟"根本没跑"无法区分
    // the start/end pair must be unconditional — when every branch skips
    // they are the only bytes run_tee lands in postfix.log, and an empty
    // file is indistinguishable from "never ran"
    log::log(&format!(
        "[extras] postfix: start — .pc metadata + archive fixups under {}",
        e.prefix
    ));

    // macOS Rust dedup（librav1e/librsvg Mach-O 符号改名）在平台模块实现
    // ——windows 的 COFF 去重在本函数尾部调 windows_deps::fixup /
    // the macOS Rust dedup lives in the platform module; Windows COFF
    // dedup is invoked at the tail of this fn via windows_deps::fixup
    let _ = work;

    // libdc1394-2 的上游 .pc 哪儿都不列 raw1394 / upstream libdc1394-2.pc
    // lists raw1394 nowhere
    libs_private_tr("libdc1394-2", &["-lraw1394"]);

    // 上游 .pc 忘记运行时的 C++ 库 / C++ libraries whose .pc forgets the
    // runtime (CXXRT is -lstdc++ on GNU toolchains, -lc++ elsewhere)
    for pc in [
        "lensfun",
        "zimg",
        "libgme",
        "libaribcaption",
        "chromaprint",
        "libchromaprint",
        "whisper",
        // libjxl 全 C++：libjxl_threads.pc 不带 C++ 运行时，--enable-libjxl
        // 显式声明下链测一死 configure 整体中止（实证：linux 的
        // std::__throw_system_error/std::thread 未解析）/ libjxl is all
        // C++: libjxl_threads.pc forgets the C++ runtime, and with an
        // explicit --enable-libjxl a failed link test aborts configure
        // outright (empirical: unresolved std::__throw_system_error /
        // std::thread on linux)
        "libjxl",
        "libjxl_threads",
        // svm.cpp 拉进 vtable/__gxx_personality 全套 / svm.cpp drags in
        // vtable and __gxx_personality references
        "libvmaf",
        // 整个 codec 是 C++（SceneChangeDetection/VAACalculation…）/
        // the whole codec is C++ (SceneChangeDetection, VAACalculation...)
        "openh264",
    ] {
        libs_private_tr(pc, &[&e.cxxrt]);
    }
    // libsrt 按配方编了 cryspr-mbedtls——.pc 只字不提 crypto 归档 /
    // libsrt compiles cryspr-mbedtls per the recipe — its .pc never
    // mentions the crypto archives
    libs_private_tr(
        "srt",
        &["-lmbedtls", "-lmbedx509", "-lmbedcrypto", &e.cxxrt],
    );
    libs_private_tr("twolame", &["-lm"]);
    // soxr WITH_OPENMP 拖入 omp_* 尾巴。只在 linux 修 .pc：windows 的
    // pacman soxr.pc 本就带 -lgomp，而 FFmpeg 对 libsoxr 是裸 -lsoxr
    // 链测不查 .pc——windows 侧的解药在 windows_common 的 extra-libs
    // （gomp 探测）；macOS 自有配方关了 OpenMP / soxr's WITH_OPENMP
    // drags omp_* tails. Patch the .pc on linux only: pacman's
    // windows soxr.pc already carries -lgomp, and FFmpeg probes
    // libsoxr with a bare -lsoxr link test that never consults a .pc —
    // the windows remedy lives in windows_common's extra-libs (the
    // gomp probe); macOS's own soxr recipe disables OpenMP
    if e.os == sys::Os::Linux {
        libs_private_tr("soxr", &["-lgomp", "-lm"]);
    }

    // macOS 的"同名同码"撞名只能靠归档手术——ld64 早把
    // -multiply_defined 打成 obsolete 直接忽略（实证：ffmpeg_g/ffplay_g
    // 链接 297 个 duplicate symbols）。两对：librav1e∩librsvg-2 嵌同一
    // rustc 的 std/alloc 单态化对象；libxevd∩libxeve 共享 mc/recon 源。
    // byte_rename 把 b 册里的交集符号等长改名（_…→z…）——引用与定义
    // 同步迁移，b 册自洽且与 a 册零碰撞（与 windows 的 redefine_sym
    // 同效，Mach-O 等价物）/
    // macOS same-name same-code collisions can only be cured by archive
    // surgery — ld64 long ago declared -multiply_defined obsolete and
    // ignores it (empirical: 297 duplicate symbols at the ffmpeg_g /
    // ffplay_g links). Two pairs: librav1e ∩ librsvg-2 embed one rustc's
    // std/alloc monomorphs; libxevd ∩ libxeve share mc/recon sources;
    // libzmq ∩ libssh collide on _sha1_init (zmq's bundled sha1.c vs
    // libssh's md_mbedcrypto adapter).
    // `nmedit -R` localizes the intersecting symbols in the second
    // archive — a local symbol escapes cross-archive resolution,
    // intra-archive references keep their own copy, and external
    // references resolve against the other archive's identical
    // definitions (same effect as windows' redefine_sym; the Mach-O
    // equivalent)
    if e.os == sys::Os::Mac {
        // 先修自洽性再 dedup：新 rustc 把 __rustc shim 发成局部符号，
        // 裸 -lrav1e 链测全家 undefined。提升为全局后它进 shared，
        // 随 byte_rename 与引用同步迁移 / fix self-consistency before
        // dedup: new rustc emits __rustc shims local so the bare -lrav1e
        // test dies; once global they enter `shared` and migrate with
        // their references under byte_rename
        for l in ["librav1e.a", "librsvg-2.a"] {
            let p = libdir.join(l);
            if !p.is_file() {
                continue;
            }
            match macho_export_shim_defs(&p) {
                Ok(0) => {}
                Ok(n) => {
                    let _ = sys::run(&e.ranlib, &[&p.to_string_lossy()], &e.work);
                    log::log(&format!(
                        "[extras] promoted {n} local def(s) to external in {l} \
                         (__rustc shim locality)"
                    ));
                }
                Err(why) => log::warn(&format!("[extras] shim promote on {l}: {why}")),
            }
        }
        // (a, b)：a 册保留原名，b 册等长改名——libzmq∩libssh 撞
        // _sha1_init（zmq 自带 sha1.c vs libssh 的 md_mbedcrypto
        // 适配器），b 册选 libssh：其 sha1_init 只是内部胶水，zmq
        // 保留规范名 / (a, b): a keeps its names, b gets renamed —
        // libzmq∩libssh collide on _sha1_init (zmq's bundled sha1.c vs
        // libssh's md_mbedcrypto adapter); b is libssh whose sha1_init
        // is only internal glue while zmq keeps the canonical name
        for (a, b) in [
            ("libxevd.a", "libxeve.a"),
            ("librsvg-2.a", "librav1e.a"),
            ("libzmq.a", "libssh.a"),
        ] {
            let (pa, pb) = (libdir.join(a), libdir.join(b));
            if !(pa.is_file() && pb.is_file()) {
                log::log(&format!(
                    "[extras] dedup pair {a}∩{b}: an archive is absent; skipped"
                ));
                continue;
            }
            // 外部定义集自解析枚举（macho_def_names）：`nm -g` 在
            // 4000+ 成员的 librsvg-2.a 上漏报——真实 shared 933 它只
            // 给 376，dedup 腰斩后 ffmpeg 终链 894 个 duplicate（实证）
            // / external-def sets come from self-parsed enumeration
            // (macho_def_names): `nm -g` under-reports on the 4000+-
            // member librsvg-2.a — 933 truly shared vs 376 emitted,
            // leaving 894 duplicates at the final ffmpeg link
            // (empirical)
            let sa = macho_def_names(&pa);
            let sb = macho_def_names(&pb);
            if sa.is_empty() || sb.is_empty() {
                log::log(&format!(
                    "[extras] dedup pair {a}∩{b}: defined sets {} vs {}; skipped",
                    sa.len(),
                    sb.len()
                ));
                continue;
            }
            let shared: Vec<String> = sa.intersection(&sb).cloned().collect();
            if shared.is_empty() {
                log::log(&format!(
                    "[extras] dedup pair {a}∩{b}: no shared symbols; skipped"
                ));
                continue;
            }
            // 等长字节改名而非 nmedit -R：-R 是把定义从符号表整个删掉
            // ——rav1e 单测 -lrav1e 里本册引用当场 undefined（实证：
            // __rustc 单态化全灭）。byte_rename 把每个共享名在 b 册
            // 改成等长新名（首字符 _ -> z，strtab 偏移原样），引用与
            // 定义同步迁移，b 册自洽且与 a 册零碰撞；nm 误报的幽灵名
            // 在字节层找不到就天然跳过，不需要重试循环 /
            // equal-length byte rename instead of nmedit -R: -R
            // deletes definitions from the symbol table outright —
            // rav1e's own references went undefined in the standalone
            // -lrav1e test (empirical: __rustc monomorphs massacred).
            // byte_rename swaps every shared name inside b for a
            // same-length new one (leading _ -> z so strtab offsets
            // stay valid); references and definitions migrate together
            // leaving b self-consistent and collision-free vs a.
            // nm's ghost reports simply never appear at byte level —
            // no retry loop needed
            let mut pairs: Vec<(String, String)> = Vec::new();
            for s in &shared {
                if !s.starts_with('_') {
                    log::warn(&format!(
                        "[extras] {a}∩{b}: shared name '{s}' lacks '_' prefix; skipped"
                    ));
                    continue;
                }
                let tgt = format!("z{}", &s[1..]);
                // 哨兵：目标名在任何一册已定义就跳过（同 windows 版）
                // sentinel: skip when the target name is already
                // defined in either archive (same rule as the windows
                // variant)
                if sa.contains(&tgt) || sb.contains(&tgt) {
                    log::warn(&format!(
                        "[extras] {a}∩{b}: skip '{s}' — target '{tgt}' already defined"
                    ));
                    continue;
                }
                pairs.push((s.clone(), tgt));
            }
            if pairs.is_empty() {
                continue;
            }
            let refs: Vec<(&str, &str)> = pairs
                .iter()
                .map(|(o, n)| (o.as_str(), n.as_str()))
                .collect();
            let n = byte_rename(&pb, &refs, &e.work)?;
            if n > 0 {
                let _ = sys::run(&e.ranlib, &[&pb.to_string_lossy()], &e.work);
            }
            log::log(&format!(
                "[extras] renamed {} {a}-shared symbol(s) in {b} across {n} member(s) \
                 (_… -> z…)",
                pairs.len()
            ));
        }
    }

    // GnuTLS 的 ARM AES 汇编与 OpenSSL aesv8-armx 同源——导出 aes_v8_* 名
    // 完全相同，静态链接撞车。等长改名 aes_v8_ -> gnaes8_
    // GnuTLS's ARM AES asm and OpenSSL's aesv8-armx come from the same
    // cryptogams sources — identical aes_v8_* exports that collide in a
    // static link. Equal-length rename aes_v8_ -> gnaes8_.
    let gntls = libdir.join("libgnutls.a");
    if gntls.is_file() && nm_has(&gntls, "aes_v8_set_encrypt_key", &e.work) {
        match byte_rename(&gntls, &[("aes_v8_", "gnaes8_")], &e.work) {
            Ok(n) if n > 0 => {
                let _ = sys::run(&e.ranlib, &[&gntls.to_string_lossy()], &e.work);
                log::log(&format!(
                    "[extras] renamed aes_v8_*->gnaes8_* in libgnutls.a ({n} members)"
                ));
            }
            _ => log::warn("could not rename aes_v8_* in libgnutls.a"),
        }
    }

    // codec2 与 speex 的 lsp.c 都导出 lpc_to_lsp/lsp_to_lpc——外来归档
    // （BtbN 的 codec2）仍带冲突名 / codec2's and speex's lsp.c both export
    // lpc_to_lsp/lsp_to_lpc — archives arriving any other way still carry
    // the clashing names
    let cdc2 = libdir.join("libcodec2.a");
    if cdc2.is_file()
        && nm_has(&cdc2, "lpc_to_lsp", &e.work)
        && !nm_has(&cdc2, "codec2_lpc_to_lsp", &e.work)
        && let Ok(n) = byte_rename(
            &cdc2,
            &[("lpc_to_lsp", "c2_lpc2lsp"), ("lsp_to_lpc", "c2_lsp2lpc")],
            &e.work,
        )
        && n > 0
    {
        let _ = sys::run(&e.ranlib, &[&cdc2.to_string_lossy()], &e.work);
        log::log(&format!(
            "[extras] namespaced lpc_to_lsp/lsp_to_lpc in libcodec2.a ({n} members)"
        ));
    }

    // libflite.a 可能缺少其头文件只声明的 extern 定义——哪个引用悬空补哪个
    // libflite.a can lack the extern definitions its headers only declare —
    // provide whichever references dangle
    let fl = libdir.join("libflite.a");
    if fl.is_file() {
        let mut src = String::new();
        if !nm_has(&fl, "c99_snprintf", &e.work) && nm_undef(&fl, "c99_snprintf", &e.work) {
            src.push_str(
                "#include <stdio.h>\n#include <stdarg.h>\nint c99_snprintf(char *str, size_t size, const char *format, ...)\n{\n    int count;\n    va_list ap;\n    va_start(ap, format);\n    count = vsnprintf(str, size, format, ap);\n    va_end(ap);\n    return count;\n}\n",
            );
        }
        if !nm_has(&fl, "ts_utf8_sequence_length", &e.work)
            && nm_undef(&fl, "ts_utf8_sequence_length", &e.work)
        {
            src.push_str(
                "int ts_utf8_sequence_length(char c0)\n{\n    return (( 0xE5000000 >> (( c0 >> 3 ) & 0x1E )) & 3 ) + 1;\n}\n",
            );
        }
        if !src.is_empty() {
            let tmp = e.work.join("fliteshim");
            let _ = std::fs::create_dir_all(&tmp);
            let c = tmp.join("fliteshim.c");
            let o = tmp.join("fliteshim.o");
            let _ = std::fs::write(&c, &src);
            if sys::run(
                &e.cc,
                &[
                    "-O2",
                    "-c",
                    &c.to_string_lossy(),
                    "-o",
                    &o.to_string_lossy(),
                ],
                &tmp,
            )
            .unwrap_or(1)
                == 0
                && sys::run(
                    &e.ar,
                    &["rcs", &fl.to_string_lossy(), &o.to_string_lossy()],
                    &tmp,
                )
                .unwrap_or(1)
                    == 0
            {
                log::log("[extras] added missing extern definitions to libflite.a");
            }
            fsx::rm_rf(&tmp);
        }
    }

    // FFmpeg 引用 <pocketsphinx/pocketsphinx.h>，上游平铺装——桥接布局
    // FFmpeg includes <pocketsphinx/pocketsphinx.h> but upstream installs it
    // flat — bridge the layout
    let pslib = libdir.join("libpocketsphinx.a");
    let psh_flat = e.prefix_win.join("include").join("pocketsphinx.h");
    let psh = e
        .prefix_win
        .join("include")
        .join("pocketsphinx")
        .join("pocketsphinx.h");
    if pslib.is_file()
        && psh_flat.is_file()
        && !psh.is_file()
        && fsx::install(&psh_flat, &psh).is_ok()
    {
        log::log("[extras] bridged pocketsphinx.h into include/pocketsphinx/");
    }
    // v5 头把 cmd_ln_t 改名 ps_config_t——重新声明旧 API（cmd_ln_free_r
    // 已删除，映射到 ps_config_free）/ v5 headers renamed cmd_ln_t to
    // ps_config_t — redeclare the old API (cmd_ln_free_r is gone, map it
    // to ps_config_free)
    if psh.is_file()
        && fsx::contains(&psh, "ps_config_t")
        && !fsx::contains(&psh, "ps_config_t cmd_ln_t")
    {
        let shim = "\n/* ffmpeg-build compat: pocketsphinx v5 renamed cmd_ln_t to ps_config_t in\n * the installed headers, but libpocketsphinx still exports cmd_ln_parse_r\n * which FFmpeg's af_asr.c calls. Re-declare the old API; cmd_ln_free_r no\n * longer exists upstream, so map it to the identical ps_config_free. */\n#ifndef FFB_POCKETSPHINX_CMDLN_COMPAT\n#define FFB_POCKETSPHINX_CMDLN_COMPAT\ntypedef ps_config_t cmd_ln_t;\ncmd_ln_t *cmd_ln_parse_r(cmd_ln_t *inout_cmdln, const ps_arg_t *defn,\n                         int argc, char *argv[], int strict);\n#define cmd_ln_free_r(config) ps_config_free(config)\n#endif /* FFB_POCKETSPHINX_CMDLN_COMPAT */\n";
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&psh) {
            let _ = f.write_all(shim.as_bytes());
            log::log("[extras] re-declared pre-v5 cmd_ln_* API on pocketsphinx/pocketsphinx.h");
        }
    }

    // leptonica 的 *io.c 引用 opj_/WebP/png/jpeg/TIFF/DGif——.pc 要拖上
    // 存在的前缀归档 / leptonica's *io.c objects reference opj_*/WebP*/
    // png_*/jpeg_*/TIFF*/DGif* — the .pc must drag the prefix archives along
    for l in [
        "openjp2", "webp", "webpmux", "sharpyuv", "png16", "png", "jpeg", "tiff", "gif", "z", "m",
    ] {
        if libdir.join(format!("lib{l}.a")).is_file() {
            libs_private_tr("lept", &[&format!("-l{l}")]);
            libs_private_tr("leptonica", &[&format!("-l{l}")]);
        }
    }

    // codec2/flite 上游不发 .pc——归档在而描述符缺席时合成最小的一份
    // neither codec2 nor flite ships a .pc upstream — synthesize a minimal
    // one when the archive is here without its descriptor
    let pcdir = libdir.join("pkgconfig");
    fsx::install_d(&pcdir)?;
    if !pcdir.join("codec2.pc").is_file() && libdir.join("libcodec2.a").is_file() {
        fsx::write(
            &pcdir.join("codec2.pc"),
            &format!(
                "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\nName: codec2\nDescription: low bit rate speech codec\nVersion: 1.2.0\nLibs: -L${{libdir}} -lcodec2\nLibs.private: -lm\nCflags: -I${{includedir}} -I${{includedir}}/codec2\n",
                e.prefix
            ),
        )?;
        log::log("[extras] wrote codec2.pc");
    }
    if !pcdir.join("flite.pc").is_file() && libdir.join("libflite.a").is_file() {
        let mut flibs = String::new();
        for f in fsx::walk(&libdir) {
            let n = f.file_name().unwrap().to_string_lossy().to_string();
            if n.starts_with("libflite_") && n.ends_with(".a") {
                flibs.push_str(&format!(" -l{}", &n[3..n.len() - 2]));
            }
        }
        fsx::write(
            &pcdir.join("flite.pc"),
            &format!(
                "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\nName: flite\nDescription: small text-to-speech engine\nVersion: 2.2\nLibs: -L${{libdir}}{flibs} -lflite\nLibs.private: -lm\nCflags: -I${{includedir}}/flite\n",
                e.prefix
            ),
        )?;
        log::log(&format!("[extras] wrote flite.pc ({flibs})"));
    }

    // windows 的重活（Requires.private 补齐、.pc 系统库修补、impshim、
    // rust_eh_personality、librsvg 自检）就是本道工序的一部分——在
    // postfix-run 子进程里跑输出才进得了 postfix.log；留在
    // extras_stage 尾部外置调用只落 CI 控制台，日志包里永远缺席
    // the heavyweight Windows pass (Requires.private top-up, .pc
    // system-lib patches, impshim, rust_eh_personality, librsvg
    // self-check) is part of this very pass — only inside the
    // postfix-run subprocess does its output land in postfix.log; called
    // separately at the tail of extras_stage it only reaches the CI
    // console and is forever absent from the log bundle
    if e.os == sys::Os::Windows {
        crate::platforms::windows_deps::fixup()?;
    }

    // 最后一道把关：合并所有重复键行（fixup 刚补的 Requires.private
    // 也在归一范围内）/ last word on .pc health: merge any repeated
    // keyword lines (including the Requires.private lines fixup just
    // appended)
    log::log("[extras] postfix: merging duplicated .pc key lines");
    pkgcfg::dedup_fields();
    log::log("[extras] postfix: done");
    Ok(())
}

/// nm 里是否有未定义引用（U 行）/ whether nm output has an undefined
/// reference (U line)
fn nm_undef(lib: &Path, sym: &str, dir: &Path) -> bool {
    sys::capture("nm", &[&lib.to_string_lossy()], dir)
        .unwrap_or_default()
        .lines()
        .any(|l| {
            let mut it = l.split_whitespace();
            it.next() == Some("U") && it.next() == Some(sym)
                || l.trim() == format!("U {sym}")
                || l.trim() == format!("U _{sym}")
        })
}
