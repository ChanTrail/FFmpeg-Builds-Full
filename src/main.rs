//! FFmpeg-Builds-Full 构建驱动程序入口
//! FFmpeg-Builds-Full build driver — entry point
//! 先构建本程序，再由它驱动各平台的 FFmpeg 原生构建。每个平台×架构
//! 一个独立建模块（platforms/<os>_<arch>.rs）。
//! Build this program first, then let it drive the native FFmpeg build
//! per platform. Every platform×arch is an independent build module
//! (platforms/<os>_<arch>.rs).
//! Usage: FFmpeg-Builds-Full <build|packages|extras|ffmpeg|package|save-deps|smoke|features|sources|help>

//! 模块布局：src/ 只留入口锚文件，功能实现全部进各功能目录。
//! Module layout: src/ holds only entry anchor files; implementations live
//! in their function directories.
mod core;
mod engine;
mod manifest;
mod platforms;
mod recipes;
mod stages;

// 根级重导出：crate::sys / crate::fsx 等旧路径继续有效，调用点零改动
// Root re-exports: crate::sys / crate::fsx etc. keep working — call sites
// untouched by the directory move
pub use core::{ar, buildsys, fsx, json, log, net, pkgcfg, sys};
pub use engine::extras;
pub use manifest::{features, patches, sources};
pub use stages::{deps, ffmpeg, smoke};

use std::process::exit;

/// 打印中英双语用法（<prog> 为 argv[0] 程序名）
/// print the bilingual usage text (<prog> is the argv[0] program name)
fn usage(prog: &str) {
    println!(
        "\
FFmpeg-Builds-Full — FFmpeg 静态构建驱动 / static FFmpeg build driver

用法 / Usage:
  {prog} <命令 / command> [参数 / args]

命令 / Commands:
  build      全流水线：extras → ffmpeg → package
             the whole pipeline: extras → ffmpeg → package
  build-all  全变体一次建：首变体（静态）走完整流水线产出 deps
             前缀，后续 shared/debug/shared-debug 变体只跑
             ffmpeg → package / every variant in one run: the first
             (static) takes the full pipeline to produce the deps
             prefix, the shared / debug / shared-debug variants then
             only re-run ffmpeg → package
  packages   安装/确认平台包栈（MSYS2 pacman 等）
             install or verify the platform package stack (MSYS2 pacman etc.)
  extras     只跑 extras 配方引擎（第三方库）
             run only the extras recipe engine (third-party libraries)
  ffmpeg     取源码 → configure → 编译 FFmpeg 本体
             fetch → configure → build FFmpeg itself
  package    打包产物并校验
             package and verify the artifact
  save-deps  保存依赖缓存
             save the dependency cache
  smoke [目录/dir]
             对目录里的 ffmpeg/ffprobe/ffplay 跑冒烟测试
             smoke-test the ffmpeg/ffprobe/ffplay in a directory
             (默认 / default: /usr/local/bin)
  features   打印本平台 features/*.yaml 解析出的特性集（调试）
             print the feature set resolved from features/*.yaml (debug)
  sources    打印本平台 sources/*.yaml 合并出的源码清单（调试）
             print the source map merged from sources/*.yaml (debug)
  help       显示本帮助 / show this help

选项 / Options（均可 = 直写，也可空格后接值 / all take --opt=value or --opt value）:
  --work <dir>     工作目录 / scratch dir   (默认 / default: ffmpeg-<os>-<arch>)
  --dist <dir>     产物输出目录 / artifact dir (默认 / default: <repo>/artifacts)
  --msys2 <dir>    MSYS2 根目录 / MSYS2 root (Windows)
  --mirror <名|基址 / name|base>
                   pacman 换源：ustc / tuna / aliyun / repo 或任意镜像基址
                   switch the pacman mirror (ustc / tuna / aliyun / repo
                   or any mirror base)
  --repo <url>     FFmpeg 仓库地址 / FFmpeg repo URL
                   (缺省读 sources/base.yaml 的 ffmpeg: 键 /
                    default: the ffmpeg: key in sources/base.yaml)
  --ref <ref>      FFmpeg 分支或 tag / FFmpeg branch or tag
  --sha <sha>      钉住的 FFmpeg commit / pinned FFmpeg commit
  --jobs <n>       并行编译数 / parallel jobs (默认 / default: CPU 核数 / cores)
  --ccache         编译经 ccache / compile through ccache
  --shared         共享构建：产物 = 可执行文件 + 共享库（bin/ 旁位自解析）
                   shared build: binaries + shared libs (sibling-resolved in bin/)
  --debug          调试构建：-g3 + 零优化 + 留符号
                   debug build: -g3 + no optimizations + keep symbols

环境变量 / Environment variables:
  每个选项都透传成同名环境变量（--work→FFBUILD_WORK 等），CI 直传 env 与
  命令行等效 / every option forwards to its namesake env var
  (--work -> FFBUILD_WORK, ...) — passing env on CI equals the flag
  GITHUB_TOKEN / GH_TOKEN、GITLAB_TOKEN / GL_TOKEN
                   git 源 CI 查询提额、私有实例 / git-source CI quota, private instances
  HTTPS_PROXY 等   代理变量，reqwest 原生支持 / proxy vars, natively honoured
  FFBUILD_CHILD / FFMPEG_GDB
                   内部管道变量（父子进程通信），非用户旋钮 /
                   internal plumbing (parent<->child IPC), not user knobs"
    );
}

/// 程序入口：help 先行（不需环境），其余命令探测平台 → 初始化环境 → 分发
/// Entry point: help first (no env needed); other commands detect the
/// platform → init env → dispatch
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let prog = args
        .first()
        .map(|s| s.rsplit(['/', '\\']).next().unwrap_or(s).to_string())
        .unwrap_or_else(|| "ffbuild".to_string());
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    // 无参数或显式求助：打印用法后直接退出，不碰平台探测
    // bare invocation or an explicit help request: print usage and exit
    // without touching platform detection
    if matches!(cmd, "help" | "-h" | "--help" | "/?" | "-?") {
        usage(&prog);
        exit(0);
    }
    // 解析命令行选项：(旗标, 透传的环境变量, 是否取值)。选项统一改写
    // 成同名环境变量后由 platforms/各阶段自取——CI workflow 直传 env
    // 与本地传参两条路殊途同归，读取侧零改动 / parse command-line
    // options as (flag, forwarded env var, takes-value): every option
    // rewrites to its namesake env var for platform stages to pick up —
    // CI workflows passing env directly and locals passing flags
    // converge on the same read sites with zero changes downstream
    const OPTS: &[(&str, &str, bool)] = &[
        ("--work", "FFBUILD_WORK", true),
        ("--dist", "FFBUILD_DIST", true),
        ("--msys2", "FFBUILD_MSYS2", true),
        ("--mirror", "FFBUILD_MIRROR", true),
        ("--repo", "FFMPEG_REPO", true),
        ("--ref", "FFMPEG_REF", true),
        ("--sha", "FFMPEG_SHA", true),
        ("--jobs", "FFBUILD_JOBS", true),
        ("--ccache", "FFBUILD_CCACHE", false),
        ("--shared", "FFBUILD_SHARED", false),
        ("--debug", "FFBUILD_DEBUG", false),
    ];
    let mut positional: Vec<String> = Vec::new();
    let mut i = 2usize;
    while i < args.len() {
        let a = &args[i];
        // --opt=value 形态先拆 / split the --opt=value form first
        let (name, inline_val) = match a.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        // -m 是 --mirror 的短别名 / -m is the short alias of --mirror
        let name = if name == "-m" { "--mirror" } else { name };
        match OPTS.iter().find(|(f, _, _)| *f == name) {
            Some((_, var, takes)) => {
                let v = if *takes {
                    match inline_val.or_else(|| {
                        i += 1;
                        args.get(i).cloned()
                    }) {
                        Some(v) => v,
                        None => log::die(&format!("{name} needs a value / {name} 需要一个值")),
                    }
                } else {
                    "1".to_string()
                };
                // SAFETY：单线程驱动，子进程尚未派生，改环境无并发读写
                // SAFETY: single-threaded driver, no children spawned
                // yet — env writes are race-free
                unsafe { std::env::set_var(var, v) };
            }
            None => positional.push(a.clone()),
        }
        i += 1;
    }
    // smoke 走轻量路径：装最小环境即可，不做平台工具链探测——verify 的
    // 干净 runner 上没有 MSYS2/工具链 / smoke takes the light path: a
    // minimal env is enough — verification runners are clean machines
    // with neither MSYS2 nor a toolchain to probe
    if cmd == "smoke" {
        sys::smoke_env();
        let dir = positional
            .first()
            .map(String::as_str)
            .unwrap_or("/usr/local/bin");
        if let Err(e) = smoke::smoke_test(std::path::Path::new(dir)) {
            log::die(&e);
        }
        exit(0);
    }
    let module: fn() -> platforms::Pipeline = match platforms::detect() {
        Some(m) => m,
        None => log::die("unsupported host platform for this driver"),
    };
    let p = module();
    if let Err(e) = (p.init_env)() {
        log::die(&e);
    }
    // 输出闸门状态显形：Linux 驱动经 sudo env 白名单透传环境变量，
    // GITHUB_ACTIONS 漏名单时这里就是 off——一眼分清"闸门没生效"与
    // "闸门没工作" / surface the output gate's state: the Linux driver
    // runs through sudo env whitelists, and a missing GITHUB_ACTIONS
    // shows up here as off — "gate disengaged" vs "gate broken" is then
    // obvious at a glance
    log::log(&format!(
        "CI output gate: {}",
        if std::env::var_os("FFBUILD_CHILD").is_some() {
            "off (child: parent run_tee gates)"
        } else if std::env::var_os("GITHUB_ACTIONS").is_some() {
            "on"
        } else {
            "off"
        }
    ));
    let rc = match cmd {
        "build" => (p.build)(),
        // build-all：变体集合一次建完——FFBUILD_SHARED/FFBUILD_DEBUG
        // 由本命令逐变体接管（外部同名旋钮被覆盖）；首变体走完整流
        // 水线产出 deps 前缀，后续变体直跑 ffmpeg→package（extras
        // 的 .ok 标记本来也会 no-op，直接跳过更干净）/ build-all
        // builds the whole variant set — this command owns
        // FFBUILD_SHARED/FFBUILD_DEBUG per variant (externally set
        // knobs get overridden); the first variant takes the full
        // pipeline to produce the deps prefix, later ones run only
        // ffmpeg→package (the extras .ok markers would no-op a
        // re-run anyway, but skipping outright is cleaner)
        "build-all" => (|| -> Result<(), String> {
            for (i, (shared, debug)) in [(false, false), (true, false), (false, true), (true, true)]
                .iter()
                .enumerate()
            {
                // SAFETY：单线程驱动，子进程尚未派生，改环境无并发读写
                // SAFETY: single-threaded driver, no children spawned
                // yet — env writes are race-free
                unsafe {
                    if *shared {
                        std::env::set_var("FFBUILD_SHARED", "1")
                    } else {
                        std::env::remove_var("FFBUILD_SHARED")
                    }
                    if *debug {
                        std::env::set_var("FFBUILD_DEBUG", "1")
                    } else {
                        std::env::remove_var("FFBUILD_DEBUG")
                    }
                }
                log::log(&format!(
                    "=== variant {i}: shared={shared} debug={debug} / 变体 {i} ==="
                ));
                if i == 0 {
                    (p.build)()?;
                } else {
                    (p.ffmpeg)()?;
                    (p.package)()?;
                }
            }
            Ok(())
        })(),
        "packages" => (p.packages)(),
        "extras" => (p.extras)(),
        "ffmpeg" => (p.ffmpeg)(),
        "package" => (p.package)(),
        "save-deps" => (p.save_deps)(),
        // 内部子命令：extras 引擎把每个配方作为子进程重入，好让 run_tee
        // 逐字落一份 <name>.log——exit code 即配方成败，故不走 log::die
        // 的 ::error:: 注解（可选配方的失败由父进程 status/warn 呈现）
        // internal subcommand: the extras engine re-enters each recipe as
        // a subprocess so run_tee can land a verbatim <name>.log — the
        // exit code IS the verdict, so it must not emit log::die's
        // ::error:: annotation (an optional failure surfaces via the
        // parent's status/warn instead)
        "recipe-run" => {
            let name = positional.first().map(String::as_str).unwrap_or_default();
            match extras::run_recipe(name) {
                Ok(()) => exit(0),
                Err(e) => {
                    eprintln!("recipe-run {name}: {e}");
                    exit(1);
                }
            }
        }
        // 同上的 postfix 重入：.pc 元数据修补/归档去重单独落
        // postfix.log，不进日志包就等于没发生过 / same re-entry for the
        // postfix pass: the .pc-metadata patch/archive dedup gets its own
        // postfix.log — without one in the bundle it never happened
        "postfix-run" => match extras::run_postfix() {
            Ok(()) => exit(0),
            Err(e) => {
                eprintln!("postfix-run: {e}");
                exit(1);
            }
        },
        "features" => features::dump(),
        "sources" => sources::dump(),
        other => {
            eprintln!("未知命令 / unknown command: '{other}'\n");
            usage(&prog);
            exit(2);
        }
    };
    if let Err(e) = rc {
        log::die(&e);
    }
    exit(0);
}
