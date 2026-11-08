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
fn byte_rename(lib: &Path, pairs: &[(&str, &str)], dir: &Path) -> Result<usize, String> {
    let tmp = dir.join(format!("bytern{}", std::process::id()));
    fsx::install_d(&tmp)?;
    let mut renamed = 0;
    for m in crate::ar::members(lib, dir) {
        if m == "/" || m == "//" || m == "__.SYMDEF" {
            continue;
        }
        // ar p 直接把成员字节倒出来——不经文件系统抽提，怪成员名也安全
        // `ar p` dumps the member bytes straight out — no filesystem
        // extraction, so exotic member names are safe
        let Ok(mut data) = crate::ar::member_data(lib, &m, dir) else {
            continue;
        };
        let hit = pairs
            .iter()
            .any(|(f, _)| data.windows(f.len()).any(|w| w == f.as_bytes()));
        if !hit {
            continue;
        }
        for (from, to) in pairs {
            debug_assert_eq!(from.len(), to.len(), "byte-rename must be equal-length");
            data = replace_bytes(&data, from.as_bytes(), to.as_bytes());
        }
        let obj = tmp.join(crate::ar::member_file(&m));
        std::fs::write(&obj, &data).map_err(|e| format!("write {obj:?}: {e}"))?;
        let fname = crate::ar::member_file(&m);
        // 成员名被消毒/截断时 ar r 不会替换原名——先删后加，杜绝同符号
        // 双份成员 / when the sanitized name differs from the original,
        // `ar r` won't replace it — delete first so no duplicate-symbol
        // member survives
        let _ = sys::run("ar", &["d", &lib.to_string_lossy(), &m], dir);
        if sys::run("ar", &["r", &lib.to_string_lossy(), &fname], &tmp).unwrap_or(1) == 0 {
            renamed += 1;
        }
    }
    fsx::rm_rf(&tmp);
    Ok(renamed)
}

/// 字节序列全量替换（等长）/ replace all occurrences of a byte sequence
/// (equal length)
fn replace_bytes(data: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    let mut i = 0;
    while i + from.len() <= out.len() {
        if &out[i..i + from.len()] == from {
            out[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
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
    ] {
        libs_private_tr(pc, &[&e.cxxrt]);
    }
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
