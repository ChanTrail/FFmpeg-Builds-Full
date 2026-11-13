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
#[derive(Clone)]
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

/// fetch_ffmpeg：克隆上游 FFmpeg 到 <dest>（blobless：完整历史+tag 供
/// N-<count>-g<hash> 版本串，只取检出 commit 的文件），写 version.sh，
/// 返回源码身份。FFMPEG_REPO/FFMPEG_REF/FFMPEG_SHA 环境变量可覆盖默认。
/// Clone upstream FFmpeg into <dest> (blobless: full history and tags for the
/// N-<count>-g<hash> version string, files of the checked-out commit
/// only), write version.sh, return the source identity. FFMPEG_REPO /
/// FFMPEG_REF / FFMPEG_SHA env vars override the defaults.
/// 注意 / note: 不打任何补丁（patches 环节按新方针整体移除）— clean
/// upstream sources only, per the rewrite policy.
pub fn fetch(dest: &Path) -> Result<Info, String> {
    // build-all 各变体的 plan 共享同一棵源码树——无条件 rm_rf+clone
    // 会一轮网拉多次；src_dir 与钉版（FFMPEG_REF/FFMPEG_SHA）在单
    // 进程内不变，记忆化等价且零代价 / build-all's variant
    // plans share one source tree — an unconditional rm_rf+clone
    // would re-pull the network per variant; src_dir and the pinned rev
    // (FFMPEG_REF/FFMPEG_SHA) cannot change inside one process, so
    // memoization is equivalent and free
    static INFO: std::sync::OnceLock<Result<Info, String>> = std::sync::OnceLock::new();
    INFO.get_or_init(|| fetch_inner(dest)).clone()
}

/// 真实取源体：克隆 + 钉版 + 烤 version.sh（见 fetch 的进程内记忆
/// 化包装）/ the real fetch body: clone + pin + bake version.sh (see
/// fetch's in-process memoizing wrapper)
fn fetch_inner(dest: &Path) -> Result<Info, String> {
    // 仓源唯一事实源是 sources/base.yaml 的 ffmpeg: 键——git: 主仓
    // + mirrors: 顺序兜底；FFMPEG_REPO 显式覆盖则单仓无回退 /
    // the manifest's ffmpeg: key (sources/base.yaml) is the single
    // source of truth — git: is the primary, mirrors: the ordered
    // fallbacks; an explicit FFMPEG_REPO overrides into a single repo
    let spec = crate::manifest::sources::spec("ffmpeg")?;
    let repos: Vec<String> = match std::env::var("FFMPEG_REPO").ok().filter(|v| !v.is_empty()) {
        Some(r) => vec![r],
        None => spec
            .git
            .iter()
            .cloned()
            .chain(spec.mirrors.iter().cloned())
            .collect(),
    };
    if repos.is_empty() {
        return Err("sources: ffmpeg entry has no git: URL".into());
    }
    let refname = std::env::var("FFMPEG_REF")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| spec.ref_.clone())
        .unwrap_or_else(|| "master".into());
    // git_run：GIT_NET 传输停滞超时 + 维护任务关闭 + 5 次递增退避
    // 重试；每仓源跑满内部重试才换下一个，半成品 dest 每次克隆前
    // 清掉 / git_run carries GIT_NET's stall timeouts and maintenance
    // off plus 5 rising-backoff retries; each repo exhausts its retries
    // before the next mirror is tried, and the partial dest is wiped
    // before every clone
    let mut last_err = String::new();
    let mut cloned = false;
    for (i, repo) in repos.iter().enumerate() {
        fsx::rm_rf(dest);
        match crate::net::git_run(
            &[
                "clone",
                "-q",
                "--filter=blob:none",
                "--branch",
                &refname,
                repo.as_str(),
                &dest.to_string_lossy(),
            ],
            &sys::env().work,
            Some(dest),
        ) {
            Ok(()) => {
                cloned = true;
                break;
            }
            Err(e) => {
                last_err = e;
                if i + 1 < repos.len() {
                    eprintln!(
                        "[fetch] clone from {repo} failed; trying the next repo / 克隆 {repo} 失败，换下一个仓源"
                    );
                }
            }
        }
    }
    if !cloned {
        return Err(format!("clone failed from every repo: {last_err}"));
    }
    // FFMPEG_SHA 钉住精确 commit：一轮的五个平台即使上游在跑期间前进
    // 也构建同一份源码 / pin the exact commit so all five platforms of a
    // run build the same source even if upstream moves mid-run
    if let Ok(sha) = std::env::var("FFMPEG_SHA")
        && !sha.is_empty()
    {
        // blobless 检出的 checkout 会惰性拉取缺失 blob——同样是网络
        // 操作，吃 GIT_NET 超时；checkout 幂等可重试无需清理 /
        // checkout on a blobless clone lazily fetches missing blobs —
        // a network op too, so it rides GIT_NET's timeouts; checkout is
        // idempotent, no cleanup between retries
        let dest_s = dest.to_string_lossy().to_string();
        crate::net::git_run(
            &[
                "-C",
                &dest_s,
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "-q",
                &sha,
            ],
            &sys::env().work,
            None,
        )?;
    }
    // 本地补丁随树上——ffmpeg 走自管克隆（不经 materialize），apply
    // 必须显式调；context 失配会响亮中止（上游漂移逼人复核，正是
    // 设计语义）/ local patches land on the tree — ffmpeg takes its
    // own clone path (not materialize), so apply must be invoked
    // explicitly; a context mismatch aborts loudly (upstream drift
    // forcing a human re-check is the designed semantics)
    crate::patches::apply(dest, "ffmpeg")?;
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

/// ffconfigure 驱动：在 <build_dir> 跑 FFmpeg configure；某个外部库缺失/损坏导致失败时
/// 把 --enable-<name> 记进 dropped-file、丢旗标再试——一次运行枚举出
/// 所有开不了的库，各带 config.log 存档。configure 最终成功后只要有
/// 一个变体的配置/编译作业：build-all 把各份 args 先算好（串行切
/// env 期间逐个 plan），configure 随后并行、编译在变体间串行 -jN
/// ——各自的 build_dir/dropped/build_log 互不干扰
/// one variant's configure/compile job: build-all plans the arg
/// sets up front (one by one while toggling the env knobs), the
/// configures then run in parallel while compiles run serially at
/// -jN across variants — each job's build_dir/dropped/build_log is
/// its own
pub struct ConfJob {
    /// 变体词（static/shared/debug/shared-debug）——日志标签与错误
    /// 归属 / the variant word (static/shared/debug/shared-debug) —
    /// log tag and error attribution
    pub label: String,
    /// out-of-tree 构建目录 / the out-of-tree build directory
    pub build_dir: PathBuf,
    /// configure 脚本 / the configure script
    pub configure: PathBuf,
    /// 完整 configure 参数集 / the full configure argument set
    pub args: Vec<String>,
    /// dropped-libs{variant}.txt / the per-variant dropped-libs file
    pub dropped: PathBuf,
    /// 必配库清单（FF_REQUIRED 语义，不带 --enable- 前缀）
    /// required libraries (FF_REQUIRED semantics, without the
    /// --enable- prefix)
    pub required: &'static [&'static str],
    /// per-变体 make 日志 / the per-variant make log
    pub build_log: PathBuf,
}

/// 从 configure 的 stdout 切出收尾汇总：最后一个 `ARCH ` 特性表行起
/// （tee 文件只留成功轮，防御性 rposition 取最后一块），到
/// `License:`/`Creating `/`WARNING` 行之前止；尾部空行剥净。取不到
/// 块时返回空串，调用方回退旧式 buildconf 内容
/// Slice the closing summary out of configure's stdout: from the last
/// `ARCH ` feature-table line (the tee file only holds the successful
/// round; the defensive rposition takes the last block anyway) up to
/// a `License:`/`Creating `/`WARNING` line, trailing blanks stripped.
/// Returns empty when no block is found — callers fall back to the
/// classic buildconf capture
pub fn conf_summary(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let Some(start) = lines.iter().rposition(|l| l.starts_with("ARCH ")) else {
        return String::new();
    };
    let mut out: Vec<String> = Vec::new();
    for l in &lines[start..] {
        let t = l.trim_start();
        if t.starts_with("License:") || t.starts_with("Creating ") || t.starts_with("WARNING") {
            break;
        }
        out.push(l.to_string());
    }
    // pod2man/makeinfo 三行在缺 doc 工具链的平台报 no——样例口径
    // 统一 yes（doc 本就 --disable-doc，检测值不影响二进制，纯属
    // 呈现归一）：只改写行尾 no→yes，保留原有列宽 / the
    // pod2man/makeinfo lines read `no` where doc tooling is absent —
    // normalize to `yes` per the sample convention (docs are
    // --disable-doc anyway; the detection value never reaches the
    // binary — presentation-only normalization): rewrite just the
    // trailing no->yes, preserving the column width
    for l in &mut out {
        for k in [
            "pod2man enabled",
            "makeinfo enabled",
            "makeinfo supports HTML",
        ] {
            if l.starts_with(k) && l.trim_end().ends_with("no") {
                let trimmed = l.trim_end().to_string();
                *l = format!("{}yes", &trimmed[..trimmed.len() - 2]);
            }
        }
    }
    while matches!(out.last(), Some(l) if l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// 从汇总文本切出启用外部库名单：External libraries: 与 External
/// libraries providing hardware acceleration: 两个三列块的所有
/// 词元，空行止 / the enabled-external-lib names from a summary: all
/// whitespace-separated tokens of the `External libraries:` and
/// `External libraries providing hardware acceleration:` three-column
/// blocks, each ending at a blank line
pub fn enabled_lib_names(conf: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    for l in conf.lines() {
        if l.starts_with("External libraries:") || l.starts_with("External libraries providing") {
            in_block = true;
            continue;
        }
        if in_block {
            if l.trim().is_empty() {
                in_block = false;
                continue;
            }
            out.extend(l.split_whitespace().map(String::from));
        }
    }
    out
}

/// 库名 → pc 模块名差异实在对不上的别名表（probe 兜底）/ pc module
/// aliases where the library name and module name just don't line up
/// (probe fallback)
const PC_ALIASES: &[(&str, &str)] = &[
    ("libmp3lame", "lame"),
    ("libopenjpeg", "libopenjp2"),
    ("libxvid", "xvidcore"),
];

/// 按名探测 pkg-config --modversion：候选 [原名, 去 lib 前缀, 下划线
/// 转连写, lib+名, 名+2, 别名]，首个非空版本即取；查不到给 None /
/// probe pkg-config --modversion for a library name: candidates
/// [name, lib-stripped, underscore->dash, lib+name, name+2, alias];
/// first non-empty version wins, None when nothing resolves
fn pc_probe(name: &str) -> Option<String> {
    let strip = name.strip_prefix("lib").unwrap_or(name);
    let mut cands: Vec<String> = vec![
        name.to_string(),
        strip.to_string(),
        name.replace('_', "-"),
        strip.replace('_', "-"),
        format!("lib{strip}"),
        format!("{strip}2"),
    ];
    if let Some((_, a)) = PC_ALIASES.iter().find(|(n, _)| *n == name || *n == strip) {
        cands.push((*a).to_string());
    }
    cands
        .into_iter()
        .map(|c| crate::pkgcfg::modversion(&c))
        .find(|v| !v.is_empty())
}

/// 装配 external libraries' versions 段：按汇总里的启用名单逐名解
/// 版本——配方行（versions.txt：.srcver/pc/spec 已选过优）→ pc 探测
/// → 配方 spec 串；都解不出的名字（mediafoundation/d3d11va 这类系统
/// API）直接丢行不造 "-" / build the external libraries' versions
/// section: resolve each name from the summary's enabled list — the
/// recipe row (versions.txt already picked .srcver/pc/spec best), a
/// pkg-config probe, then the recipe spec string; names resolving to
/// nothing (system APIs like mediafoundation/d3d11va) are dropped
/// rather than emitted as "-"
pub fn enabled_versions(conf: &str) -> String {
    let map = crate::extras::versions_map();
    let table = crate::recipes::table();
    let mut rows: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for n in enabled_lib_names(conf) {
        // 配方匹配：名字直接相等、lib+配方名、或 flags 带 --enable-<名>
        // / recipe match: name equality, lib+recipe-name, or a flags
        // field carrying --enable-<name>
        let rec = table.iter().find(|r| {
            r.name == n
                || n == format!("lib{}", r.name)
                || r.flags
                    .split([',', ' '])
                    .any(|f| f == format!("--enable-{n}"))
        });
        let rec_row = rec
            .and_then(|r| map.get(r.name))
            .filter(|v| !v.trim().is_empty() && v.trim() != "-");
        let v = rec_row.cloned().or_else(|| pc_probe(&n)).or_else(|| {
            rec.map(|r| crate::sources::spec_version(r.name))
                .filter(|v| !v.is_empty())
        });
        if let Some(v) = v {
            rows.insert(n, v);
        }
    }
    rows.iter()
        .map(|(n, v)| format!("{n} {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 丢包仍返回 Err：被要求开启却悄悄缺库的 ffmpeg 绝不能发布。
/// Drive FFmpeg configure in <build_dir>; when it fails on a missing/broken external
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
    label: &str,
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
    // 变体标签插进 [ffconfigure] 前缀——并行配置下多路 note 交错，
    // 行内标签是唯一的归属线索 / the variant tag rides inside the
    // [ffconfigure] prefix — under parallel configures the
    // note streams interleave and the in-line tag is the only
    // attribution
    let tag = if label.is_empty() {
        String::new()
    } else {
        format!(":{label}")
    };
    let note = |msg: &str| {
        let msg = msg.replacen("[ffconfigure]", &format!("[ffconfigure{tag}]"), 1);
        println!("{msg}");
        let _ = fsx::append_line(&fflog, &msg);
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
                        // 关闭一个 FFmpeg 本就没有的选项不构成能力缺失；而
                        // enable 被丢是"请求了却交付不了"：enabled 清单此刻
                        // 仍声称它已启用，库也白编了，必须升级警告（原先只
                        // 发 note + dropped-libs.txt，BUILDINFO 事后才看得
                        // 到）/ only a dropped --enable-* counts as a
                        // missing library — asking to disable an option
                        // FFmpeg never had removes no requested capability;
                        // a dropped --enable-* means a requested feature
                        // silently stays off while the enabled list still
                        // claims it and the dep was built for nothing —
                        // escalate to warn (it used to be note-only plus a
                        // dropped-libs.txt line only visible in BUILDINFO)
                        if kind == "--enable-" {
                            log::warn(&format!(
                                "feature '{base}' was enabled but this FFmpeg has no \
                                 '--enable-{opt}' option; the flag was dropped and \
                                 it stays off"
                            ));
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

    // configure stdout tee 落盘：样例版 BUILDINFO 的 build
    // configuration 段要的是 configure 收尾的汇总（特性表+Enabled
    // 清单），-buildconf 新版只吐旗标。run_tee 每轮截断——最后一轮
    // （成功轮）的输出留在文件里 / tee configure's stdout to disk:
    // the sample BUILDINFO's build-configuration section wants
    // configure's closing summary (the feature table + Enabled
    // lists), while modern -buildconf emits flags only. run_tee
    // truncates every round — the last (successful) round's output
    // is what stays
    let confout = dropped_file.with_extension("confout.txt");
    const MAX_ROUNDS: u32 = 60;
    let mut round = 0;
    loop {
        round += 1;
        note(&format!(
            "[ffconfigure] round {round} with {} arguments",
            args.len()
        ));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if sys::run_tee(&cfg, &refs, build_dir, &confout).unwrap_or(1) == 0 {
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

/// 并行跑一批变体的 configure：探针是 fork/链接密集（大头是
/// 几百个小编译的进程派生开销而非 CPU——MSYS2 上尤甚），多路并行
/// ≈ 单倍时长；各变体的 dropped/fflog/config-log 分文件落盘，
/// note 行内带变体标签归属 / run a batch of variant configures
/// in parallel: probing is fork/link-bound (the cost is the
/// process spawn of hundreds of tiny compiles, not CPU — doubly so
/// under MSYS2) so parallel ≈ a single variant's wall time; each
/// variant's dropped/fflog/config-log lands in its own files and
/// note lines carry the in-line variant tag
pub fn configure_all(jobs: &[ConfJob]) -> Result<(), String> {
    let run_one = |j: &ConfJob| -> Result<(), String> {
        let refs: Vec<&str> = j.args.iter().map(String::as_str).collect();
        configure(
            &j.build_dir,
            &j.configure,
            &refs,
            &j.dropped,
            j.required,
            &j.label,
        )
    };
    if jobs.len() == 1 {
        return run_one(&jobs[0]);
    }
    let results = std::thread::scope(|s| {
        jobs.iter()
            .map(|j| s.spawn(move || run_one(j)))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("configure thread panicked".into()))
            })
            .collect::<Vec<_>>()
    });
    // 按变体序归错——并行丢错也要带名字 / attribute failures in
    // variant order — parallel errors still need a name on them
    for (j, r) in jobs.iter().zip(results) {
        r.map_err(|e| format!("{}: {e}", j.label))?;
    }
    Ok(())
}

/// 在 <build_dir> 编译 FFmpeg（tee 到 <log_file>）
/// compile FFmpeg in <build_dir> (tee'd to <log_file>)
pub fn compile(build_dir: &Path, log_file: &Path) -> Result<(), String> {
    let e = sys::env();
    let jobs = e.jobs.to_string();
    match sys::run_tee("make", &["-j", &jobs], build_dir, log_file)? {
        0 => Ok(()),
        rc => {
            // CI 下 run_tee 不回显子进程输出——失败时把日志尾部全量打到
            // 控制台，否则只剩干巴巴的 "make exited N" / run_tee does
            // not echo child output under CI — print the verbatim log
            // tail on failure, else all we report is "make exited N"
            println!("{}", sys::log_tail(log_file, 80));
            Err(format!("make exited {rc}"))
        }
    }
}

/// `make install`（install-doc 尽力而为，失败退纯 install）
/// `make install` (install-doc is best-effort, falls back to plain
/// install)
pub fn install(build_dir: &Path, log_file: &Path) -> Result<(), String> {
    // install-doc 是锦上添花；失败退回纯 install / install-doc is a nice
    // to have; fall back to plain install
    if sys::run_tee("make", &["install", "install-doc"], build_dir, log_file)? != 0
        && sys::run_tee("make", &["install"], build_dir, log_file)? != 0
    {
        // 同上的失败尾部全量 / same verbatim tail on install failure
        println!("{}", sys::log_tail(log_file, 80));
        return Err("make install failed".to_string());
    }
    Ok(())
}

/// 单变体完整构建 = 编译 + 安装 / a single-variant full build =
/// compile + install
pub fn build(build_dir: &Path, log_file: &Path) -> Result<(), String> {
    compile(build_dir, log_file)?;
    install(build_dir, log_file)
}

/// 需要外部资产的特性 → (资产名, 获取地址)。运行时 dlopen 插件或读数据
/// 文件的特性才进包提示；静态编进的库不算资产
/// features that need external assets -> (asset label, fetch URL). Only
/// runtime-dlopen plugins and runtime-read data files qualify; libs
/// statically linked in are not assets
const EXTERNAL_ASSETS: &[(&str, &str, &str)] = &[
    // frei0r 只编宿主头文件——插件本体在上游 frei0r 仓（标准插件集即
    // 该仓所产）/ frei0r builds only the host headers — the plugins
    // themselves live in the upstream frei0r repo (the standard plugin
    // set is what it produces)
    ("frei0r", "frei0r plugins", "https://github.com/dyne/frei0r"),
    // lensfun 配方只产库——XML 数据库在上游仓 data/db（也是
    // lensfun-update-data 的抓取源）/ the lensfun recipe ships the
    // library alone — the XML database lives in upstream's data/db
    // (the same source lensfun-update-data pulls from)
    (
        "liblensfun",
        "lensfun database",
        "https://github.com/lensfun/lensfun",
    ),
    (
        "whisper",
        "whisper models",
        "https://huggingface.co/ggerganov/whisper.cpp/tree/main",
    ),
    (
        "libtesseract",
        "tesseract data",
        "https://github.com/tesseract-ocr/tessdata",
    ),
    (
        "pocketsphinx",
        "pocketsphinx models",
        "https://github.com/cmusphinx/pocketsphinx/tree/master/model",
    ),
    ("ladspa", "LADSPA plugins", "https://www.ladspa.org/"),
    ("lv2", "LV2 plugins", "https://lv2plug.in/"),
];

/// 由 -buildconf 的 configuration 行拼 External Assets 段：以实际启用
/// 旗标为准（平台级 disable 的特性自动缺席）；全灭时连段名都不留
/// build the External Assets section from the -buildconf configuration
/// line, driven by the flags that were actually enabled (platform-level
/// disables simply absent); the whole section is omitted when none hit
pub fn external_assets(buildconf: &str) -> String {
    let mut out = String::new();
    for (flag, label, url) in EXTERNAL_ASSETS {
        // 整词匹配——frei0r 这类短名防子串误配 / whole-token match —
        // short names like frei0r must not substring-collide
        let want = format!("--enable-{flag}");
        if buildconf
            .split_whitespace()
            .any(|t| t.trim_end_matches(',') == want)
        {
            // "label:" 左对齐到 22 列挂 URL（样例版式；最长标签
            // "pocketsphinx models:" 20 字符，18 列会黏连）/ "label:"
            // left-aligned to column 22 before the URL (sample
            // layout; the longest label "pocketsphinx models:" is 20
            // chars — column 18 would glue it to the URL)
            out.push_str(&format!("{:<22}{url}\n", format!("{label}:")));
        }
    }
    if out.is_empty() {
        return out;
    }
    // 尾部空行随段带走——调用方直接内嵌，空段连分隔空行一并消失 /
    // the trailing blank line rides with the section — callers inline
    // it verbatim and an empty section takes its separator along
    format!("External Assets\n{out}\n")
}

/// 把构建目录里未剥离副本（ffmpeg_g[.exe]，回退 ffmpeg[.exe]）的
/// nm -n 符号表落到 <work>/ffmpeg{variant}.nm——verify 的 gdb 回溯
/// 全是 ??（strip 后地址不可辨）；此表只随失败日志包上传供离线对
/// 号，不进发布包。best-effort：文件缺席或 nm 失败都静默略过 /
/// emit the unstripped twin's (ffmpeg_g[.exe], falling back to
/// ffmpeg[.exe]) nm -n symbol table to <work>/ffmpeg{variant}.nm —
/// verify's gdb backtraces are all ?? (stripped addresses
/// unresolvable); the table rides only the failure log bundle for
/// offline resolution, never the release package. best-effort: a
/// missing file or a failed nm is silently skipped
pub fn emit_symbols(work: &Path, variant: &str) {
    let bd = work.join(format!("ffbuild-ffmpeg{variant}"));
    for cand in ["ffmpeg_g", "ffmpeg_g.exe", "ffmpeg", "ffmpeg.exe"] {
        let f = bd.join(cand);
        if !f.is_file() {
            continue;
        }
        if let Ok(text) = sys::capture("nm", &["-n", &f.to_string_lossy()], work) {
            // .nm.txt 扩展名：collect-logs 只收 *.txt/*.log——裸 .nm 会
            // 被过滤掉（实证）/ the .nm.txt suffix: collect-logs only
            // gathers *.txt/*.log — a bare .nm gets filtered out
            // (empirical)
            let _ = fsx::write(&work.join(format!("ffmpeg{variant}.nm.txt")), &text);
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::{conf_summary, enabled_lib_names};

    /// 样例形态：ARCH 特性表起、Enabled 清单、License/Creating 收尾 /
    /// the sample shape: an ARCH feature table, Enabled lists, and a
    /// License/Creating tail
    const CONFOUT: &str = "install prefix            /d/a/ffbuild/prefix\n\
        C compiler                gcc\n\
        ARCH                      x86 (generic)\n\
        big-endian                no\n\
        runtime cpu detection     yes\n\
        \n\
        External libraries:\n\
        bzlib                   libxcb                  zlib\n\
        \n\
        Enabled decoders:\n\
        aac                     h264                    hevc\n\
        \n\
        Enabled outdevs:\n\
        sdl2\n\
        \n\
        License: GPL version 3 or later\n\
        Creating config.mak, config.h, and doc/config.texi...\n";

    #[test]
    fn slices_from_arch_to_before_license() {
        let s = conf_summary(CONFOUT);
        assert!(s.starts_with("ARCH"));
        assert!(s.contains("Enabled outdevs:"));
        assert!(s.contains("External libraries:"));
        assert!(!s.contains("install prefix"));
        assert!(!s.contains("License:"));
        assert!(!s.contains("Creating"));
        assert!(!s.ends_with('\n'));
    }

    #[test]
    fn takes_last_arch_block_when_rounds_accumulate() {
        let two_rounds = format!("ARCH stale\nLicense: x\n{CONFOUT}");
        let s = conf_summary(&two_rounds);
        assert!(!s.contains("stale"));
        assert!(s.contains("Enabled decoders:"));
    }

    #[test]
    fn empty_when_no_arch_block() {
        assert!(conf_summary("configure: error: foo\n").is_empty());
    }

    #[test]
    fn normalizes_doc_tool_lines_to_yes() {
        // 真实特性表形态：pod2man no、makeinfo yes——no 变 yes 且列宽
        // 不动，texi2html 不碰 / the real feature-table shape: pod2man
        // no, makeinfo yes — no flips to yes with the column width
        // intact, texi2html untouched
        let raw = "ARCH                      x86 (generic)\n\
                   texi2html enabled         no\n\
                   perl enabled              yes\n\
                   pod2man enabled           no\n\
                   makeinfo enabled          yes\n\
                   makeinfo supports HTML    no\n\
                   experimental features     yes\n\
                   License: GPL version 3 or later\n";
        let s = conf_summary(raw);
        assert!(s.contains("pod2man enabled           yes"));
        assert!(s.contains("makeinfo enabled          yes"));
        assert!(s.contains("makeinfo supports HTML    yes"));
        assert!(s.contains("texi2html enabled         no"));
    }

    #[test]
    fn enabled_names_cover_both_blocks() {
        let s = "External libraries:\n\
                 avisynth                libjxl                  libx264\n\
                 gmp                     libmp3lame              zlib\n\
                 \n\
                 External libraries providing hardware acceleration:\n\
                 amf                     d3d11va                 vulkan\n\
                 \n\
                 Libraries:\n\
                 avcodec                 avutil\n";
        let names = enabled_lib_names(s);
        assert_eq!(names.len(), 9);
        assert!(names.contains(&"libmp3lame".to_string()));
        assert!(names.contains(&"vulkan".to_string()));
        // Libraries:/Programs: 块不算外部库 / Libraries:/Programs:
        // blocks are not external libraries
        assert!(!names.contains(&"avcodec".to_string()));
    }
}
