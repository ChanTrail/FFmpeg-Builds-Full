//! extras 引擎：配方表驱动——跳过不适用平台的项，按
//! provided/cached/需构建三分支处理，跑配方、收尾 postfix、落状态文件。
//! 完全自包含的 Rust 实现，不调用任何旧 .sh。
//! Extras engine: drive the recipe table — skip entries that don't apply to
//! this platform, classify each as provided/cached/to-build, run recipes,
//! postfix, write status files. Self-contained Rust; no legacy .sh invoked.
//!
//! 与旧引擎的两处刻意不同 / Deliberate differences from the old engine:
//! * FFmpeg 旗标不再由配方收集——features/*.json 是旗标真源，配方表里的
//!   flags 字段只作归属记录（仍写 flags.txt 备查）
//!   FFmpeg flags are no longer collected from recipes — features/*.json is
//!   the flag source of truth; the table's flags field is provenance only
//!   (flags.txt still written for the record)
//! * 不跑补丁环节——先保持干净源码构建（patches 留 old/ 备查）
//!   no patch phase — clean upstream sources only for now (patches stay in
//!   old/ for reference)

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

/// extras_provided：平台自带栈已把库装进前缀（pc 模块必须来自前缀内，
/// 静态归档门槛也要过）/ the platform's own stack already installed the
/// library into the prefix (pc modules must come from inside the prefix and
/// the static-archive gate must pass)
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
        // .pc 文件必须位于 $PREFIX 或 $MSYSTEM_PREFIX 内部——系统 -dev 包
        // 不算"已提供" / the .pc file must live inside $PREFIX or
        // $MSYSTEM_PREFIX — a system -dev package is not "provided"
        let pcd = pkgcfg::variable(c, "pcfiledir");
        let inside = [&e.prefix, &e.env_prefix]
            .iter()
            .any(|p| !p.is_empty() && pcd.starts_with(p.as_str()));
        if !inside {
            return false;
        }
    }
    pkgcfg::static_libs_ok(check).is_ok()
}

/// 配方 .ok 标记的期望哈希：配方分组源码 + 工具链指纹
/// Expected .ok hash: group source + toolchain fingerprint
pub fn recipe_sum(r: &Recipe) -> Result<String, String> {
    let tc = crate::deps::toolchain();
    sys::sha256_data(format!("{}{tc}", r.src).as_bytes())
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
    }

    // 先修好重复的 .pc 键行——恢复的前缀可能带着重复 Libs.private 的
    // .pc，它会死在 extras_provided 的 pkg-config --exists 上
    // heal duplicated .pc keyword lines first — a restored prefix can carry
    // a repeated Libs.private that dies inside extras_provided's
    // `pkg-config --exists`
    pkgcfg::dedup_fields();

    let status = state.join("status.txt");
    let flagsf = state.join("flags.txt");
    let mut failed: Vec<String> = Vec::new();

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

        // 配方体是进程内 Rust 函数：子命令输出直通本进程日志（旧版按
        // 配方落 .log 文件的做法不再需要——CI 折叠组已经提供了隔离）
        // the recipe body is an in-process Rust fn: child output streams to
        // our own log (the old per-recipe .log is unnecessary — CI fold
        // groups already isolate it)
        let rc = (r.run)(&ctx);

        if rc.is_ok()
            && pkgcfg::check_spec(r.check).is_ok()
            && pkgcfg::static_libs_ok(r.check).is_ok()
        {
            fsx::write(&okf, &format!("{rsum}\n"))?;
            if r.flags != "-" {
                fsx::append_line(&flagsf, r.flags)?;
            }
            fsx::append_line(&status, &format!("{} ok", r.name))?;
            log::log(&format!("[extras] {}: ok", r.name));
        } else {
            if let Err(e) = rc {
                println!("recipe error: {e}");
            }
            fsx::append_line(&status, &format!("{} failed", r.name))?;
            log::warn(&format!(
                "optional library '{}' failed to build and is left out of this build",
                r.name
            ));
            failed.push(r.name.to_string());
        }
        log::group_end();
        fsx::rm_rf(&wd);
    }

    if let Err(e) = postfix() {
        fsx::append_line(&status, "postfix failed")?;
        let _ = std::fs::copy(&status, work.join("extras-status.txt"));
        let _ = std::fs::copy(&flagsf, work.join("extras-flags.txt"));
        return Err(format!(
            "archive post-processing failed, refusing to configure FFmpeg: {e}"
        ));
    }

    // 本趟跑完了（各配方结果在 status.txt）/ the pass ran to completion
    fsx::write(&state.join("done"), "")?;
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
        return Err(format!(
            "required libraries failed to build, refusing to continue: {}",
            failed.join(" ")
        ));
    }
    Ok(())
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

/// 等长记号在归档成员里做字节级改名（perl 的 -0777pi 移植成纯 Rust）
/// Equal-length byte rename inside every archive member that carries the
/// token (the perl -0777pi pass ported to plain Rust)
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

/// extras_postfix：对已装归档和 .pc 文件做去重/修补——针对单个配方看不
/// 见的链接期问题 / dedup/fixup pass over the installed archives and pc
/// files for link-time problems no single recipe can see
fn postfix() -> Result<(), String> {
    let e = sys::env();
    let libdir = e.prefix_win.join("lib");
    let work = e.work.clone();

    // macOS Rust dedup（librav1e/librsvg Mach-O 符号改名）在平台模块实现
    // ——windows 的 COFF 去重走 deps_fixup / the macOS Rust dedup lives in
    // the platform module; Windows COFF dedup happens in deps_fixup
    let _ = work;

    // libdc1394-2 的上游 .pc 哪儿都不列 raw1394 / upstream libdc1394-2.pc
    // lists raw1394 nowhere
    pkgcfg::libs_private("libdc1394-2", &["-lraw1394"]);

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
        pkgcfg::libs_private(pc, &[&e.cxxrt]);
    }
    pkgcfg::libs_private("twolame", &["-lm"]);
    // soxr WITH_OPENMP 拖入 GOMP_* ——仅 GCC 运行时（windows/clangarm64/
    // macOS 跳过）/ soxr drags GOMP_* when built WITH_OPENMP — GCC runtimes
    // only (skipped on windows/clangarm64/macOS)
    if e.os == sys::Os::Linux {
        pkgcfg::libs_private("soxr", &["-lgomp", "-lm"]);
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
            pkgcfg::libs_private("lept", &[&format!("-l{l}")]);
            pkgcfg::libs_private("leptonica", &[&format!("-l{l}")]);
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

    // 最后一道把关：合并所有重复键行 / last word on .pc health: merge any
    // repeated keyword lines
    pkgcfg::dedup_fields();
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
