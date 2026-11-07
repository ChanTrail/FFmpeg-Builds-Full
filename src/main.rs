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

/// 打印中英双语用法 / print the bilingual usage text
fn usage(prog: &str) {
    println!(
        "\
FFmpeg-Builds-Full — FFmpeg 静态构建驱动 / static FFmpeg build driver

用法 / Usage:
  {prog} <命令 / command> [参数 / args]

命令 / Commands:
  build      全流水线：extras → ffmpeg → package
             the whole pipeline: extras → ffmpeg → package
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
  选项 / Options:
  --mirror <名|基址 / name|base>
             pacman 换源：ustc / tuna / aliyun / repo 或任意镜像基址，
             改写 mirrorlist.* 里的 mirror.msys2.org（默认不改）
             switch the pacman mirror: ustc / tuna / aliyun / repo or any
             mirror base; rewrites mirror.msys2.org in mirrorlist.*
             (left untouched by default)
  smoke [目录/dir]
             对目录里的 ffmpeg/ffprobe/ffplay 跑冒烟测试
             smoke-test the ffmpeg/ffprobe/ffplay in a directory
             (默认 / default: /usr/local/bin)
  features   打印本平台 features/*.yaml 解析出的特性集（调试）
             print the feature set resolved from features/*.yaml (debug)
  sources    打印本平台 sources/*.yaml 合并出的源码清单（调试）
             print the source map merged from sources/*.yaml (debug)
  help       显示本帮助 / show this help

环境变量 / Environment variables:
  FFBUILD_WORK   工作目录 / scratch dir      (默认 / default: ffmpeg-<os>-<arch>)
  FFBUILD_DIST   产物输出目录 / artifact dir  (默认 / default: <repo>/artifacts)
  FFBUILD_MSYS2  MSYS2 根目录 / MSYS2 root    (Windows)
  FFBUILD_MIRROR pacman 镜像基址 / pacman mirror base (同 / as --mirror)
  FFMPEG_REPO    FFmpeg 仓库地址 / FFmpeg repo URL
  FFMPEG_REF     FFmpeg 分支或 tag / FFmpeg branch or tag
  FFMPEG_SHA     钉住的 FFmpeg commit / pinned FFmpeg commit"
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
    // 解析命令行选项：--mirror[=]<名|基址> 透传成 FFBUILD_MIRROR 环境
    // 变量，platforms 各阶段自取；其余非选项参数收进 positional。
    // Parse command-line options: --mirror[=]<name|base> is forwarded as
    // the FFBUILD_MIRROR env var for platform stages to pick up; other
    // non-option args are collected into positional.
    let mut positional: Vec<String> = Vec::new();
    let mut i = 2usize;
    while i < args.len() {
        let a = &args[i];
        if let Some(v) = a.strip_prefix("--mirror=") {
            // SAFETY：单线程驱动，子进程尚未派生，改环境无并发读写
            // SAFETY: single-threaded driver, no children spawned yet
            unsafe { std::env::set_var("FFBUILD_MIRROR", v) };
        } else if a == "--mirror" || a == "-m" {
            i += 1;
            match args.get(i) {
                Some(v) => unsafe { std::env::set_var("FFBUILD_MIRROR", v) },
                None => log::die("--mirror needs a value / --mirror 需要一个值"),
            }
        } else {
            positional.push(a.clone());
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
    let rc = match cmd {
        "build" => (p.build)(),
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
