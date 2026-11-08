//! 依赖持久化存储：由固定 "deps" GitHub release 支撑的持久化依赖库。
//! 库前缀构建一次后被每一轮复用，正常一轮只编译 FFmpeg 本体。
//! Durable dependency store backed by a fixed "deps" GitHub release: the
//! library prefix is built once and reused by every run, so a normal run
//! only compiles FFmpeg itself.
//! 产物名 / Asset names:
//!   deps-<platform>-<timestamp>.tar.zst   完整前缀快照（每平台保留最新 3 个）
//!                                         complete prefix snapshot (3 newest kept)
//!   deps-<platform>-partial.tar.zst       最新未完成前缀（断点续跑超时的构建）
//!                                         latest in-progress prefix (resume)
//! GH_TOKEN 或 gh CLI 缺席时全部是空操作，脚本在 Actions 之外也能工作。
//! Everything is a no-op without GH_TOKEN or the gh CLI, so the pipeline
//! also works outside Actions.

use crate::fsx;
use crate::log;
use crate::sys;
use std::path::Path;

/// release 名 / the release tag
const DEPS_RELEASE: &str = "deps";

/// deps_toolchain：C 工具链的一行指纹，折叠进每个 .ok/.stage-ok 标记哈希。
/// 编译器升级必须让缓存归档失效：gcc LTO 字节码是锁版本的
/// One-line fingerprint of the C toolchain, folded into every .ok/.stage-ok
/// marker hash. A compiler bump must invalidate cached archives: gcc LTO
/// bytecode is version-locked.
pub fn toolchain() -> String {
    let e = sys::env();
    sys::capture(&e.cc, &["--version"], &e.work)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or("")
        .to_string()
}

/// deps_key：输入串 sha256 前 16 个十六进制位。存在的文件按内容哈希，
/// 其余按字符串哈希 / first 16 hex chars of sha256 over the inputs —
/// existing files by content, anything else as a string
pub fn key(items: &[&str]) -> String {
    let mut data = Vec::new();
    for f in items {
        let p = Path::new(f);
        if p.is_file() {
            data.extend(std::fs::read(p).unwrap_or_default());
        } else {
            data.extend(format!("{f}\n").as_bytes());
        }
    }
    sys::sha256_data(&data)
        .unwrap_or_default()
        .chars()
        .take(16)
        .collect()
}

/// gh CLI + token + 仓库环境是否齐备 / whether gh CLI + token + repo env
/// are all present
pub fn have_gh() -> bool {
    (std::env::var("GH_TOKEN").is_ok() || std::env::var("GITHUB_TOKEN").is_ok())
        && std::env::var("GITHUB_REPOSITORY").is_ok()
        && sys::which("gh").is_some()
}

/// gh 命令（固定 --repo），stdout/stderr 双双接管——`gh release view`/
/// `gh api` 的应答动辄整屏 JSON 与 markdown 表格，曾在 Publish deps 步
/// 造成巨量转储。成功返回捕获的 stdout；失败把 stderr 一并塞进 Err，
/// 诊断不丢。`gh api` 不带 --repo 走 `gh_quiet(.., false)`。
/// run gh with the repo flag pinned and both streams captured — `gh
/// release view` / `gh api` replies are screens of JSON and markdown
/// tables that used to flood the Publish deps step. On success return
/// captured stdout; on failure stderr rides inside the Err so
/// diagnostics are not lost. `gh api` takes no --repo; use
/// `gh_quiet(.., false)` there.
fn gh_silent(args: &[&str], dir: &Path) -> Result<String, String> {
    gh_quiet(args, dir, true)
}

/// gh_quiet：gh_silent 的底层——with_repo=false 时原样发参数（`gh api`
/// 的 endpoint 自带 repos/<repo> 路径）/ the workhorse: with_repo=false
/// sends the args verbatim (`gh api` endpoints already carry the
/// repos/<repo> path)
fn gh_quiet(args: &[&str], dir: &Path, with_repo: bool) -> Result<String, String> {
    let mut a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    if with_repo {
        a.push("--repo".into());
        a.push(std::env::var("GITHUB_REPOSITORY").unwrap_or_default());
    }
    let refs: Vec<&str> = a.iter().map(String::as_str).collect();
    let mut c = sys::cmd("gh", &refs);
    c.current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let out = c.output().map_err(|e| format!("spawn gh {args:?}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "gh {args:?} exited {:?}: {}",
            out.status.code(),
            stderr.trim()
        ));
    }
    Ok(stdout)
}

/// deps_ensure_release：确保 deps release 存在（并把描述按实时产物清单
/// 重渲染一次，产物轮换后说明永不过期）
/// Ensure the deps release exists; re-render the bilingual title/body with
/// the live asset list so the description never goes stale
pub fn ensure_release() -> Result<(), String> {
    let e = sys::env();
    if !have_gh() {
        return Err("no gh".into());
    }
    if gh_silent(&["release", "view", DEPS_RELEASE], &e.work).is_err() {
        // 首次保存可能两平台抢建；输家落到下面验证 / first save ever: two
        // platforms may race the create; the loser falls through to the
        // verification below
        if let Err(m) = gh_silent(
            &[
                "release",
                "create",
                DEPS_RELEASE,
                "--latest=false",
                "--title",
                "build dependencies",
                "--notes",
                "Auto-managed dependency prefixes for the CI builds. Safe to delete; they are rebuilt.",
            ],
            &e.work,
        ) {
            log::log(&format!("deps release create: {m}"));
        }
        if gh_silent(&["release", "view", DEPS_RELEASE], &e.work).is_err() {
            return Err("deps release unavailable".into());
        }
    }
    // 重渲染标题/正文：这个 release 是缓存桶不是产品，戴不得 Latest 徽章
    // re-render title/body: this release is a cache bucket, not a product —
    // it must never wear the Latest badge
    let assets = gh_silent(
        &[
            "release",
            "view",
            DEPS_RELEASE,
            "--json",
            "assets",
            "-q",
            ".assets[] | \"\\(.name)\"",
        ],
        &e.work,
    )
    .unwrap_or_default();
    let mut lines: Vec<String> = assets.lines().map(|s| s.to_string()).collect();
    lines.sort();
    // 正文对齐线上格式：双语说明 + 「滚动续跑点」表（partial，单格一行）
    // + 「完成快照」表（时间戳名，每平台最新 3 个、最新在上、缺位 —）。
    // 产物名按 deps-<platform>-* 归类，下载 URL 直接拼。
    // Body matches the live format: bilingual blurb + a "Rolling resume
    // points" table (partials, one row) + a "Complete snapshots" table
    // (timestamped names, 3 newest per platform, newest on top, `—` for
    // gaps). Assets are grouped by deps-<platform>-* and download URLs
    // are composed directly.
    const PLATS: [&str; 5] = [
        "linux-arm64",
        "linux-x86_64",
        "macos-arm64",
        "windows-arm64",
        "windows-x86_64",
    ];
    let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let link = |name: &str| -> String {
        if name.is_empty() {
            "—".into()
        } else {
            format!("[{name}](https://github.com/{repo}/releases/download/{DEPS_RELEASE}/{name})")
        }
    };
    let mut partial_cells: Vec<String> = Vec::new();
    let mut completes: Vec<Vec<String>> = Vec::new();
    for p in PLATS {
        let want = format!("deps-{p}-partial.tar.zst");
        partial_cells.push(if lines.contains(&want) {
            link(&want)
        } else {
            "—".into()
        });
        let mut c: Vec<String> = lines
            .iter()
            .filter(|n| {
                n.starts_with(&format!("deps-{p}-"))
                    && n.ends_with(".tar.zst")
                    && !n.ends_with("-partial.tar.zst")
            })
            .cloned()
            .collect();
        // 时间戳名 → 名序即时序 / timestamped names: name order == time order
        c.sort_by(|a, b| b.cmp(a));
        c.truncate(3);
        completes.push(c);
    }
    let mut snapshot_rows = String::new();
    let rows = completes.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..rows {
        let cells: Vec<String> = completes
            .iter()
            .map(|c| link(c.get(i).map(String::as_str).unwrap_or("")))
            .collect();
        snapshot_rows.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    let header = PLATS.join(" | ");
    let body = format!(
        "自动管理的依赖前缀缓存 —— 删除安全，CI 会按需重建。\n\
         Auto-managed dependency prefixes — safe to delete; the CI rebuilds them on demand.\n\n\
         ### 滚动续跑点 / Rolling resume points\n\n\
         有进展但未全绿时的最新前缀，每轮覆盖；恢复后按标记续跑。\n\
         Latest incomplete prefix per platform, clobbered each dirty run; restore resumes via markers.\n\n\
         | {header} |\n|---|---|---|---|---|\n| {} |\n\n\
         ### 完成快照 / Complete snapshots\n\n\
         FFmpeg 构建前全部脚本与库构建成功时打的全量前缀 —— 时间戳命名，每平台保留最近 3 个（最新的在上）。\n\
         Full prefix published only when every build step before the FFmpeg compile was green — timestamp-named, 3 newest kept per platform (newest on top).\n\n\
         | {header} |\n|---|---|---|---|---|\n{snapshot_rows}",
        partial_cells.join(" | ")
    );
    if let Ok(rel_id) = gh_quiet(
        &[
            "api",
            &format!(
                "repos/{}/releases/tags/{DEPS_RELEASE}",
                std::env::var("GITHUB_REPOSITORY").unwrap_or_default()
            ),
            "-q",
            ".id",
        ],
        &e.work,
        false,
    ) {
        let rel_id = rel_id.trim().to_string();
        if !rel_id.is_empty() {
            // 应答是整个 release JSON——静默跑，失败才留一行 / the reply
            // is the whole release JSON — run silently, log only failures
            if let Err(m) = gh_quiet(
                &[
                    "api",
                    "-X",
                    "PATCH",
                    &format!(
                        "repos/{}/releases/{rel_id}",
                        std::env::var("GITHUB_REPOSITORY").unwrap_or_default()
                    ),
                    "-f",
                    "make_latest=false",
                    "-f",
                    "name=构建依赖缓存 / Build dependencies cache",
                    "-f",
                    &format!("body={body}"),
                ],
                &e.work,
                false,
            ) {
                log::log(&format!("deps release body refresh: {m}"));
            }
        }
    }
    Ok(())
}

/// deps_find_asset：回显该平台最新可用产物名（按上传时间）/ echo the
/// platform's newest asset name (by upload time)
pub fn find_asset(platform: &str) -> Result<String, String> {
    let e = sys::env();
    ensure_release()?;
    let assets = gh_silent(
        &[
            "release",
            "view",
            DEPS_RELEASE,
            "--json",
            "assets",
            "-q",
            ".assets[] | \"\\(.updatedAt)\\t\\(.name)\"",
        ],
        &e.work,
    )?;
    let mut rows: Vec<&str> = assets.lines().collect();
    rows.sort_by(|a, b| b.cmp(a)); // 最新在前 / newest first
    for r in rows {
        let name = r.split('\t').nth(1).unwrap_or("");
        if name.starts_with(&format!("deps-{platform}-")) && name.ends_with(".tar.zst") {
            return Ok(name.to_string());
        }
    }
    Err("no asset".into())
}

/// deps_fetch：下载 <platform> 最佳产物到 destdir，回显文件名
/// download <platform>'s best asset into destdir and echo its name
pub fn fetch(platform: &str, destdir: &Path) -> Result<String, String> {
    let e = sys::env();
    let name = find_asset(platform)?;
    fsx::install_d(destdir)?;
    gh_silent(
        &[
            "release",
            "download",
            DEPS_RELEASE,
            "-p",
            &name,
            "-D",
            &destdir.to_string_lossy(),
            "--clobber",
        ],
        &e.work,
    )
    .map_err(|e| format!("gh download failed: {e}"))?;
    Ok(name)
}

/// deps_publish <platform> <restored> <tarball> <complete> <dirty>：
/// 判定发布什么 + 保留策略（每平台最新 3 个 complete + 滚动 partial）
/// Decide what to publish + retention (3 newest completes + rolling
/// partial per platform)
pub fn publish(
    platform: &str,
    restored: &str,
    tarball: &Path,
    complete: bool,
    dirty: bool,
) -> Result<(), String> {
    let e = sys::env();
    if ensure_release().is_err() {
        log::warn("GH_TOKEN/gh missing; deps prefix not published");
        return Ok(());
    }
    let partial = format!("deps-{platform}-partial.tar.zst");
    let mut name = String::new();
    if !restored.is_empty() && restored != partial && !dirty {
        log::log("deps prefix unchanged since restored snapshot; nothing to upload");
    } else if complete {
        // UTC 时间戳 / UTC timestamp
        let ts = sys::capture_bash("date -u +%Y%m%dT%H%M%SZ", &e.work)
            .unwrap_or_default()
            .trim()
            .to_string();
        name = format!("deps-{platform}-{ts}.tar.zst");
    } else if dirty {
        name = partial.clone();
    } else {
        log::log("deps prefix unchanged and incomplete; skipping upload");
    }
    if !name.is_empty() {
        let size = std::fs::metadata(tarball).map(|m| m.len()).unwrap_or(0);
        if size == 0 {
            log::warn(&format!("deps tarball {tarball:?} missing; not published"));
        } else if size >= 1_900_000_000 {
            // GitHub 拒绝大于 2 GiB 的产物 / GitHub refuses assets > ~2 GiB
            log::warn(&format!(
                "deps tarball is {size} bytes (>= ~2 GiB limit); not published"
            ));
        } else {
            log::log(&format!("uploading deps asset {name} ({size} bytes)"));
            // `gh release upload` 的产物名取文件基名——先把文件起成产物名
            // （硬链接，失败则复制）/ the asset name is always the file
            // basename — give the file the asset name on disk first
            let udir = tarball.parent().unwrap().join(".deps-asset");
            fsx::install_d(&udir)?;
            let upath = udir.join(&name);
            if std::fs::hard_link(tarball, &upath).is_err() {
                std::fs::copy(tarball, &upath).map_err(|e| format!("cp: {e}"))?;
            }
            let up = gh_silent(
                &[
                    "release",
                    "upload",
                    DEPS_RELEASE,
                    "--clobber",
                    &upath.to_string_lossy(),
                ],
                &e.work,
            );
            fsx::rm_rf(&udir);
            match up {
                Ok(_) => log::log(&format!("published deps asset {name}")),
                Err(m) => log::warn(&format!("deps upload failed: {m}")),
            }
        }
    }
    // 保留策略：每平台最新 3 个 complete + 滚动 partial；再老的删除
    // retention: keep the 3 newest completes and the rolling partial
    let assets = gh_silent(
        &[
            "release",
            "view",
            DEPS_RELEASE,
            "--json",
            "assets",
            "-q",
            ".assets[] | \"\\(.updatedAt)\\t\\(.name)\"",
        ],
        &e.work,
    )
    .unwrap_or_default();
    let mut completes: Vec<String> = assets
        .lines()
        .map(|l| l.split('\t').nth(1).unwrap_or("").to_string())
        .filter(|n| {
            n.starts_with(&format!("deps-{platform}-"))
                && n.ends_with(".tar.zst")
                && !n.ends_with("-partial.tar.zst")
        })
        .collect();
    completes.sort_by(|a, b| b.cmp(a));
    for old in completes.iter().skip(3) {
        log::log(&format!("pruning old deps snapshot {old}"));
        if let Err(m) = gh_silent(
            &["release", "delete-asset", DEPS_RELEASE, "-y", old],
            &e.work,
        ) {
            log::warn(&format!("delete-asset {old}: {m}"));
        }
    }
    // 平台键改名（plat(): mac-arm64 → macos-arm64）前上传的遗留产物：
    // 正文表里没有它们的格子，find_asset 也不会取——见一个清一个
    // leftover assets uploaded before the platform-key rename (plat():
    // mac-arm64 -> macos-arm64): the body table has no cell for them and
    // find_asset never picks them — sweep on sight
    for old in assets
        .lines()
        .map(|l| l.split('\t').nth(1).unwrap_or(""))
        .filter(|n| n.starts_with("deps-mac-arm64-"))
    {
        log::log(&format!("sweeping legacy deps asset {old}"));
        if let Err(m) = gh_silent(
            &["release", "delete-asset", DEPS_RELEASE, "-y", old],
            &e.work,
        ) {
            log::warn(&format!("delete-asset {old}: {m}"));
        }
    }
    let _ = gh_silent(
        &[
            "release",
            "delete-asset",
            DEPS_RELEASE,
            "-y",
            "deps-upload.tar.zst",
        ],
        &e.work,
    );
    // 让 tag 指向最近塑造这个桶的 commit（refs 不存在时 PATCH 会报错——
    // 尽力而为，只留一行日志）/ keep the tag pointing at the commit whose
    // run last shaped the bucket (the PATCH fails when the ref does not
    // exist — best effort, one log line only)
    if let Ok(sha) = std::env::var("GITHUB_SHA") {
        let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_default();
        if let Err(m) = gh_quiet(
            &[
                "api",
                "-X",
                "PATCH",
                &format!("repos/{repo}/git/refs/tags/{DEPS_RELEASE}"),
                "-f",
                &format!("sha={sha}"),
                "-F",
                "force=true",
            ],
            &e.work,
            false,
        ) {
            log::log(&format!("deps tag re-point: {m}"));
        }
    }
    let _ = ensure_release();
    Ok(())
}

/// deps_setup_ccache：ccache 在 MSYS2 runner 上双重不可用（WoA 仿真
/// rdrand 失败 + 冒名 shim 弄坏 gcc libexec 推导）——Windows 的复用由
/// deps release 提供，本函数恒为空操作 / ccache is unusable on the MSYS2
/// runners, twice over — Windows gets its reuse from the deps release
/// instead; always a no-op here
pub fn setup_ccache() -> Result<(), String> {
    let e = sys::env();
    if e.os == sys::Os::Windows {
        log::log("ccache shims disabled on MSYS2 (masquerade breaks gcc and ninja spawns)");
        return Err("ccache disabled".into());
    }
    Err("ccache not implemented on this platform".into())
}

/// 前缀下任何已装 pacman 包拥有的文件（./relative 形式）的占位实现——
/// Windows 专属逻辑在平台模块 / placeholder for the pacman-ownership
/// manifest — the windows-only logic lives in the platform module
pub fn pacman_files(_prefix: &Path) -> Vec<String> {
    Vec::new()
}
