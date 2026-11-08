//! FFmpeg 本体：克隆源码、写 version.sh、驱动 configure（丢包重试）、
//! 编译安装。平台无关——旗标由平台模块组装后传入。
//! FFmpeg itself: clone the source, bake version.sh, drive configure
//! (drop-and-retry), compile and install. Platform-agnostic — the platform
//! module assembles the flag list.

use crate::fsx;
use crate::log;
use crate::sys;
use std::path::{Path, PathBuf};

/// 构建产物信息 / build identity
pub struct Info {
    /// 完整 commit sha / full commit sha
    pub commit: String,
    /// commit 前 10 位 / first 10 hex of the commit
    pub hash: String,
    /// version.sh 输出的版本串（已按 artifact 安全字符过滤）
    /// version string from version.sh (filtered to artifact-safe chars)
    pub version: String,
    /// 源码目录 / source dir
    pub src: PathBuf,
}

/// fetch_ffmpeg：克隆上游 FFmpeg（blobless：完整历史+tag 供
/// N-<count>-g<hash> 版本串，只取检出 commit 的文件），写 version.sh，
/// 返回源码身份。FFMPEG_REPO/FFMPEG_REF/FFMPEG_SHA 环境变量可覆盖默认。
/// Clone upstream FFmpeg (blobless: full history and tags for the
/// N-<count>-g<hash> version string, files of the checked-out commit
/// only), write version.sh, return the source identity. FFMPEG_REPO /
/// FFMPEG_REF / FFMPEG_SHA env vars override the defaults.
/// 注意 / note: 不打任何补丁（patches 环节按新方针整体移除）— clean
/// upstream sources only, per the rewrite policy.
pub fn fetch(dest: &Path) -> Result<Info, String> {
    let repo = std::env::var("FFMPEG_REPO")
        .unwrap_or_else(|_| "https://github.com/FFmpeg/FFmpeg.git".into());
    let refname = std::env::var("FFMPEG_REF").unwrap_or_else(|_| "master".into());
    fsx::rm_rf(dest);
    sys::run_ok(
        "git",
        &[
            // 一次性检出禁维护任务（见 core/net.rs 同款注释）
            // maintenance off for throwaway checkouts (see core/net.rs)
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
            "clone",
            "--filter=blob:none",
            "--branch",
            &refname,
            &repo,
            &dest.to_string_lossy(),
        ],
        &sys::env().work,
    )?;
    // FFMPEG_SHA 钉住精确 commit：一轮的五个平台即使上游在跑期间前进
    // 也构建同一份源码 / pin the exact commit so all five platforms of a
    // run build the same source even if upstream moves mid-run
    if let Ok(sha) = std::env::var("FFMPEG_SHA")
        && !sha.is_empty()
    {
        sys::run_ok(
            "git",
            &[
                "-C",
                &dest.to_string_lossy(),
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "-q",
                &sha,
            ],
            &sys::env().work,
        )?;
    }
    let commit = sys::capture(
        "git",
        &["-C", &dest.to_string_lossy(), "rev-parse", "HEAD"],
        &sys::env().work,
    )?
    .trim()
    .to_string();
    let hash = commit.chars().take(10).collect::<String>();
    // 把发布版本直接烤进 ffbuild/version.sh：configure 的 --extra-version
    // 只能在上游前缀后追加，永远替换不了。脚本契约保持：无 $2 打印版本、
    // $2 写 version.h、$3=gut 写带守卫的 ffversion.h。
    // Bake the release version straight into ffbuild/version.sh:
    // --extra-version can only append to the upstream prefix. Contract
    // kept: no $2 prints the version, $2 writes version.h, $3=gut writes
    // the guarded ffversion.h.
    let date = sys::capture_bash("date -u +%Y%m%d", &sys::env().work)
        .unwrap_or_default()
        .trim()
        .to_string();
    let verstring = format!("{date}-{hash}-build-full-www.chantrail.com");
    let vs = dest.join("ffbuild/version.sh");
    fsx::write(
        &vs,
        &format!(
            "#!/bin/sh\nif [ -z \"$2\" ]; then\n  printf '%s\\n' \"{verstring}\"\nelif [ \"$3\" = gut ]; then\n  printf '#ifndef AVUTIL_FFVERSION_H\\n#define AVUTIL_FFVERSION_H\\n#define FFMPEG_VERSION \"%s\"\\n#endif\\n' \"{verstring}\" >\"$2\"\nelse\n  printf '#define FFMPEG_VERSION \"%s\"\\n' \"{verstring}\" >\"$2\"\nfi\n"
        ),
    )?;
    let version = sys::capture_bash(
        &format!(
            "sh '{}' \"$0\" 2>/dev/null | tr -c 'A-Za-z0-9._\\n-' '-'",
            sys::win_to_posix(&vs, &sys::env().msys2_root)
        ),
        &sys::env().work,
    )
    .unwrap_or_else(|_| verstring.clone())
    .trim()
    .to_string();
    log::log(&format!(
        "FFmpeg {version} (commit {commit}, name hash {hash})"
    ));
    Ok(Info {
        commit,
        hash,
        version,
        src: dest.to_path_buf(),
    })
}

/// ffconfigure 驱动：跑 FFmpeg configure；某个外部库缺失/损坏导致失败时
/// 把 --enable-<name> 记进 dropped-file、丢旗标再试——一次运行枚举出
/// 所有开不了的库，各带 config.log 存档。configure 最终成功后只要有
/// 丢包仍返回 Err：被要求开启却悄悄缺库的 ffmpeg 绝不能发布。
/// Drive FFmpeg configure; when it fails on a missing/broken external
/// library, record --enable-<name> in dropped-file, drop the flag and
/// retry — one run enumerates EVERY library that cannot be enabled, each
/// with its config.log. Even when configure finally succeeds, drops make
/// this an Err: an ffmpeg silently missing requested libraries must never
/// ship.
/// `required` 里的库（FF_REQUIRED 语义，不带 --enable- 前缀）让枚举提前
/// 中止 / names in `required` abort the enumeration early
pub fn configure(
    build_dir: &Path,
    configure: &Path,
    args: &[&str],
    dropped_file: &Path,
    required: &[&str],
) -> Result<(), String> {
    fsx::write(dropped_file, "")?;
    // 清掉残留的 per-library config log——warm/deps 缓存会把它们无限
    // 携带，读起来和本轮一样 / stale per-library config logs ride the
    // caches forever — clear before fresh drops write new ones
    let stem = dropped_file.with_extension("");
    if let Some(dir) = stem.parent() {
        for f in fsx::walk(dir) {
            let n = f.file_name().unwrap().to_string_lossy().to_string();
            if n.starts_with(&format!("{}-", stem.file_name().unwrap().to_string_lossy()))
                && n.ends_with(".config-log.txt")
            {
                let _ = std::fs::remove_file(&f);
            }
        }
    }
    // 持久化进度日志：每行都镜像到文件 / persistent progress log
    let fflog = dropped_file.with_extension("fflog.txt");
    fsx::write(&fflog, "")?;
    let note = |msg: &str| {
        println!("{msg}");
        let _ = fsx::append_line(&fflog, msg);
    };

    /// 兼容别名 → 规范名（FFmpeg 接受但 --help 不列出的拼法）
    /// compat alias → canonical name (spellings FFmpeg accepts but --help
    /// doesn't list)
    fn alias(base: &str) -> Option<&'static str> {
        match base {
            "fontconfig" => Some("libfontconfig"),
            _ => None,
        }
    }
    let mut args: Vec<String> = args
        .iter()
        .map(|a| {
            for kind in ["--enable-", "--disable-"] {
                if let Some(rest) = a.strip_prefix(kind) {
                    let (opt, tail) = match rest.find('=') {
                        Some(i) => (&rest[..i], &rest[i..]),
                        None => (rest, ""),
                    };
                    let base = opt.replace('_', "-");
                    if let Some(canon) = alias(&base) {
                        return format!("{kind}{canon}{tail}");
                    }
                }
            }
            a.to_string()
        })
        .collect();
    note(&format!(
        "[ffconfigure] starting; {} args; configure={}; cwd={}",
        args.len(),
        configure.display(),
        build_dir.display()
    ));

    // 手上的 FFmpeg 完全不认识的选项会让 configure 以 'Unknown option'
    // 中止——提前丢掉 / options this FFmpeg doesn't know abort configure
    // with 'Unknown option' — drop them up front
    let cfg = configure.to_string_lossy().to_string();
    let help = sys::capture(&cfg, &["--help"], build_dir).unwrap_or_default();
    if !help.is_empty() {
        args.retain(|a| {
            for kind in ["--enable-", "--disable-"] {
                if let Some(rest) = a.strip_prefix(kind) {
                    let opt = rest.split('=').next().unwrap_or(rest);
                    let base = opt.replace('_', "-");
                    // help 每类选项只印一个方向：外部库只印 --enable-X、
                    // autodetect 库只印 --disable-X，但两个方向 configure
                    // 都认——按名字任一方向命中即视为已知 /
                    // help prints only one direction per option class:
                    // external libs show --enable-X, autodetect libs show
                    // --disable-X — yet configure accepts either, so match
                    // the option name in either direction
                    let known = ["--enable-", "--disable-"].iter().any(|k| {
                        [
                            format!("{k}{base} "),
                            format!("{k}{base}="),
                            format!("{k}{base}["),
                        ]
                        .iter()
                        .any(|p| help.contains(p.trim_end()))
                    });
                    if !known {
                        note(&format!(
                            "[ffconfigure] this FFmpeg has no option '{kind}{opt}'; skipping it"
                        ));
                        // 只有被丢掉的 --enable-* 才计入"缺失的库"——要求
                        // 关闭一个 FFmpeg 本就没有的选项不构成能力缺失 /
                        // only a dropped --enable-* counts as a missing
                        // library — asking to disable an option FFmpeg
                        // never had removes no requested capability
                        if kind == "--enable-" {
                            let _ =
                                fsx::append_line(dropped_file, &format!("{base} (unknown option)"));
                        }
                        return false;
                    }
                }
            }
            true
        });
    }

    const MAX_ROUNDS: u32 = 60;
    let mut round = 0;
    loop {
        round += 1;
        note(&format!(
            "[ffconfigure] round {round} with {} arguments",
            args.len()
        ));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if sys::run(&cfg, &refs, build_dir).unwrap_or(1) == 0 {
            // 体检：退出 0 却没产出 ffbuild/config.mak 的 configure 并没有
            // 真正完成 / sanity: exit 0 without ffbuild/config.mak did not
            // really finish
            if !build_dir.join("ffbuild/config.mak").is_file() {
                note(
                    "[ffconfigure] configure exited 0 but ffbuild/config.mak is missing — treating as failure",
                );
                return Err("configure silent failure".into());
            }
            let dropped = fsx::read(dropped_file);
            note(&format!(
                "[ffconfigure] configure succeeded (dropped: {})",
                dropped.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
            if !dropped.trim().is_empty() {
                note(&format!(
                    "::error::FFmpeg configure could not enable: {}",
                    dropped.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
                note("::error::refusing to build an ffmpeg missing requested libraries");
                return Err("configure could not enable all requested libraries".into());
            }
            return Ok(());
        }
        // 从最后一个 ERROR 行取库名 / pull the library name from the last
        // ERROR line
        let clog = build_dir.join("ffbuild/config.log");
        let err = fsx::read(&clog)
            .lines()
            .rfind(|l| l.starts_with("ERROR: "))
            .map(String::from)
            .unwrap_or_default();
        note(&format!(
            "[ffconfigure] configure failed: {}",
            if err.is_empty() {
                "<no ERROR line found>"
            } else {
                &err
            }
        ));
        if err.is_empty() {
            return Err("configure failed with no ERROR line".into());
        }
        // 名字字符集必须含 '.'（librsvg-2.0 等 pkg-config 名）
        // the name charset must include '.' (pkg-config names like
        // librsvg-2.0 carry it)
        let name = {
            let rest = err.trim_start_matches("ERROR: ");
            let tok: String = rest
                .chars()
                .take_while(|c| {
                    c.is_ascii_alphanumeric() || *c == '_' || *c == '+' || *c == '.' || *c == '-'
                })
                .collect();
            tok.replace('_', "-").to_lowercase()
        };
        if name.is_empty() {
            return Err("configure failed; unparseable ERROR line".into());
        }
        // pkg-config 名带 ABI 后缀：libdc1394-2 -> libdc1394
        // pc names carry an ABI suffix the flag drops
        let altname = {
            let mut n = name.clone();
            while let Some(pos) = n.rfind('-') {
                let tail = &n[pos + 1..];
                if tail.chars().all(|c| c.is_ascii_digit() || c == '.') && !tail.is_empty() {
                    n.truncate(pos);
                } else {
                    break;
                }
            }
            n
        };
        // 绝不丢必需库 / never drop a required library
        for req in required {
            if req.replace('_', "-") == name {
                note(&format!(
                    "[ffconfigure] '{name}' is required, not dropping it"
                ));
                return Err(format!("required library '{name}' failed"));
            }
        }
        // 从参数里删 --enable-<name>（两种拼法 + 去版本后缀的）
        // remove --enable-<name> (both spellings + the version-stripped one)
        let mut found = false;
        args.retain(|a| {
            let norm = a.replace('_', "-");
            let hit = norm == format!("--enable-{name}")
                || norm == format!("--enable-lib{name}")
                || norm == format!("--enable-{altname}")
                || norm == format!("--enable-lib{altname}");
            found |= hit;
            !hit
        });
        if !found {
            note(&format!(
                "[ffconfigure] '{name}' is not enabled through an explicit flag; cannot drop it"
            ));
            return Err(format!("cannot drop '{name}'"));
        }
        let _ = fsx::append_line(dropped_file, &name);
        // 把这轮的 config.log 留在丢包清单旁边（下一轮会覆盖）
        // keep this round's config.log next to the dropped list (the next
        // round overwrites it)
        if clog.is_file() {
            let dst = stem.parent().unwrap().join(format!(
                "{}-{name}.config-log.txt",
                stem.file_name().unwrap().to_string_lossy()
            ));
            let _ = std::fs::copy(&clog, &dst);
        }
        log::warn(&format!(
            "FFmpeg configure dropped optional library '{name}' (see the job summary)"
        ));
        let _ = fsx::append_line(
            &fflog,
            &format!("FFmpeg configure dropped optional library '{name}' (see the job summary)"),
        );
        if round >= MAX_ROUNDS {
            return Err("configure exceeded the round cap".into());
        }
    }
}

/// 编译并安装 FFmpeg（tee 到日志文件）/ compile and install FFmpeg
/// (tee'd to the log file)
pub fn build(build_dir: &Path, log_file: &Path) -> Result<(), String> {
    let e = sys::env();
    let jobs = e.jobs.to_string();
    match sys::run_tee("make", &["-j", &jobs], build_dir, log_file)? {
        0 => {}
        rc => return Err(format!("make exited {rc}")),
    }
    // install-doc 是锦上添花；失败退回纯 install / install-doc is a nice
    // to have; fall back to plain install
    if sys::run_tee("make", &["install", "install-doc"], build_dir, log_file)? != 0 {
        sys::run_tee("make", &["install"], build_dir, log_file)?;
    }
    Ok(())
}
