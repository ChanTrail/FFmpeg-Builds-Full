//! macOS arm64 平台共享助手：brew 工具链、deps 工作区快照（整目录
//! tar.zst）、FFmpeg 编排与打包。唯一入口 mac_arm64 使用——对应旧
//! scripts/macos/build.sh（markus-perl vendored 生态已弃用：统一走本
//! 仓 Rust 配方）。
//! Shared helpers for macOS arm64: the brew toolchain, deps workspace
//! snapshots (whole-directory tar.zst), FFmpeg orchestration and
//! packaging. The single mac_arm64 entry uses it — the counterpart of
//! the old scripts/macos/build.sh (the markus-perl vendored ecosystem is
//! retired: this repo's Rust recipes drive every platform uniformly).

use crate::fsx;
use crate::log;
use crate::sys::{self, Arch, Env, Os};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 工作根目录：$RUNNER_TEMP/ffmpeg-macos（旧 WORK）/ the work root:
/// $RUNNER_TEMP/ffmpeg-macos (the old WORK)
pub fn work_root() -> PathBuf {
    std::env::var("RUNNER_TEMP")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("ffmpeg-macos")
}

/// 库前缀：工作区快照整树打包它（.extras/ 标记在树内）。runner 用户对
/// /opt 无写权限，所以前缀放 RUNNER_TEMP 下——与旧 WS_CACHE 等价
/// The library prefix: deps snapshots tar this whole tree (the .extras/
/// markers ride inside). The runner user cannot write /opt, so the
/// prefix lives under RUNNER_TEMP — the old WS_CACHE equivalent
pub fn prefix_dir() -> PathBuf {
    std::env::var("RUNNER_TEMP")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("ffmpeg-macos-workspace")
}

/// FFmpeg 默认安装前缀（configure 不传 --prefix）/ FFmpeg's default
/// install prefix (configure gets no --prefix)
pub const USRLOCAL: &str = "/usr/local";

/// 探测并安装全局环境 / probe and install the global Env
pub fn init_env(arch: Arch) -> Result<(), String> {
    let bash = PathBuf::from("/bin/bash");
    if !bash.is_file() {
        return Err("/bin/bash not found".into());
    }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let prefix = prefix_dir();
    let work = std::env::var("FFBUILD_WORK")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(work_root);
    let dist = std::env::var("FFBUILD_DIST")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("artifacts"));
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // 子进程 PATH：前缀 bin + homebrew + 系统 PATH / child PATH: prefix
    // bin + homebrew + the system PATH
    let old_path = std::env::var("PATH").unwrap_or_default();
    let child_path = format!(
        "{}/bin:/opt/homebrew/bin:/usr/local/bin:{old_path}",
        prefix.to_string_lossy()
    );
    sys::set_env(Env {
        os: Os::Mac,
        arch,
        msystem: String::new(),
        pkgprefix: String::new(),
        pacrepo: String::new(),
        msys2_root: PathBuf::from("/"),
        prefix: prefix.to_string_lossy().to_string(),
        env_prefix: prefix.to_string_lossy().to_string(),
        prefix_win: prefix,
        bash,
        cc: "clang".into(),
        cxx: "clang++".into(),
        ar: "ar".into(),
        ranlib: "ranlib".into(),
        nm: "nm".into(),
        strip: "strip".into(),
        objdump: "otool".into(),
        cxxrt: "-lc++".into(),
        gomp: RwLock::new(String::new()),
        jobs,
        work,
        dist,
        repo_root,
        child_path,
        inside_msys: false,
    });
    Ok(())
}

/// packages/macos.txt 行：每行一个 brew 包名 / packages/macos.txt rows:
/// one brew package per line
fn package_rows() -> Vec<String> {
    include_str!("../../packages/macos.txt")
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

/// packages 阶段：brew 装 autotools 系（配方从零 autoreconf；runner 镜像
/// 不带 autotools，brew 的 libtool 是 g- 前缀的）+ `brew link --force
/// gettext`。失败不致命——旧脚本 `|| true` 语义保留：镜像漂移时单包
/// 缺失不拖死构建 / the packages stage: brew installs the autotools
/// family (recipes autoreconf from scratch; the runner image ships none
/// and brew's libtool is g-prefixed) + `brew link --force gettext`.
/// Failures are non-fatal — the old script's `|| true` semantics stay:
/// a package missing on a drifting image must not stop the build
pub fn packages() -> Result<(), String> {
    let e = sys::env();
    // work/prefix/dist 在任何子进程以它们为 cwd 前就得在场——首个
    // run() 的 current_dir 撞不存在的目录会以 "spawn prog: ENOENT" 收场
    // work/prefix/dist must exist before any child uses them as cwd —
    // the first run()'s current_dir on a missing dir dies as
    // "spawn prog: ENOENT"
    fsx::install_d(&e.work)?;
    fsx::install_d(&prefix_dir())?;
    fsx::install_d(&e.dist)?;
    if sys::which("brew").is_none() {
        log::warn("brew not found; assuming the runner image already provides the toolchain");
        return Ok(());
    }
    log::group_begin("Install brew tools");
    let pkgs = package_rows();
    let refs: Vec<&str> = pkgs.iter().map(String::as_str).collect();
    let mut args = vec!["install"];
    args.extend(&refs);
    if sys::run("brew", &args, &e.work)? != 0 {
        for p in &refs {
            let _ = sys::run("brew", &["install", p], &e.work);
        }
    }
    let _ = sys::run("brew", &["link", "--force", "gettext"], &e.work);
    log::group_end();
    Ok(())
}

/// deps 签名：配方表 + sources/features 清单 + 工具链指纹 / the deps
/// signature: recipe table + sources/features manifests + the toolchain
/// fingerprint
fn deps_sig() -> String {
    let e = sys::env();
    let recipes = crate::recipes::table()
        .iter()
        .map(|r| format!("{}:{}", r.name, r.src))
        .collect::<Vec<_>>()
        .join("\n");
    crate::deps::key(&[
        &recipes,
        &e.repo_root.join("sources").to_string_lossy(),
        &e.repo_root.join("features").to_string_lossy(),
        &crate::deps::toolchain(),
        e.plat().as_str(),
    ])
}

/// extras 阶段入口：恢复 deps 工作区 → 按签名复验标记 → 跑配方表
/// The extras stage entry: restore the deps workspace -> revalidate
/// markers against the signature -> run the recipe table
pub fn extras_stage() -> Result<(), String> {
    restore()?;
    crate::extras::run_all()
}

/// restore_deps 移植：拉本平台 deps 产物解进 RUNNER_TEMP → .deps-sig
/// 不符时清整表完成标记（每个 .ok 自带配方哈希自会复验）
/// Port of restore_deps: pull this platform's deps asset and untar into
/// RUNNER_TEMP -> on a .deps-sig mismatch drop the whole-table done
/// marker (each .ok carries its recipe hash and revalidates itself)
pub fn restore() -> Result<(), String> {
    let e = sys::env();
    let ddir = e.work.join("deps-dl");
    fsx::install_d(&ddir)?;
    let restored = match crate::deps::fetch(&e.plat(), &ddir) {
        Ok(a) => a,
        Err(_) => {
            log::log("no deps snapshot available; building the workspace from scratch");
            return Ok(());
        }
    };
    log::group_begin(&format!("Restore deps workspace ({restored})"));
    let parent = prefix_dir().parent().unwrap().to_path_buf();
    sys::bash_ok(
        &format!(
            "tar -C '{}' -xf '{}'",
            parent.to_string_lossy(),
            ddir.join(&restored).to_string_lossy()
        ),
        &e.work,
    )?;
    fsx::write(&e.work.join("deps-restored"), &restored)?;

    let sig_file = prefix_dir().join(".deps-sig");
    if fsx::read(&sig_file).trim() != deps_sig() {
        log::log("deps workspace was built for different inputs; revalidating recipe markers");
        let state = prefix_dir().join(".extras");
        let _ = std::fs::remove_file(state.join("done"));
        let _ = std::fs::remove_file(state.join("status.txt"));
        let _ = std::fs::remove_file(state.join("flags.txt"));
    }
    fsx::write(&sig_file, &format!("{}\n", deps_sig()))?;
    log::group_end();
    Ok(())
}

/// save_deps 移植：工作区整树 tar -19 → deps::publish（complete =
/// .extras/done 在场）/ port of save_deps: whole-workspace `tar -19` ->
/// deps::publish (complete = the .extras/done marker exists)
pub fn save() -> Result<(), String> {
    let e = sys::env();
    let platform = e.plat().to_string();
    let ws = prefix_dir();
    if !ws.is_dir() {
        log::log("no workspace to publish");
        return Ok(());
    }
    let complete = ws.join(".extras/done").is_file();
    let restored = fsx::read(&e.work.join("deps-restored")).trim().to_string();
    let dirty = false;
    if !restored.is_empty() && restored != format!("deps-{platform}-partial.tar.zst") && !dirty {
        log::log("deps workspace unchanged since restored snapshot; skipping pack+upload");
        return Ok(());
    }
    if !crate::deps::have_gh() {
        log::warn("GH_TOKEN/gh missing; skipping deps upload");
        return Ok(());
    }
    let tarball = e.work.join("deps-upload.tar.zst");
    sys::bash_ok(
        &format!(
            "tar -C '{}' -I 'zstd -19 -T0' -cf '{}' '{}'",
            ws.parent().unwrap().to_string_lossy(),
            tarball.to_string_lossy(),
            ws.file_name().unwrap().to_string_lossy()
        ),
        &e.work,
    )?;
    crate::deps::publish(&platform, &restored, &tarball, complete, dirty)?;
    let _ = std::fs::remove_file(&tarball);
    Ok(())
}

/// FFmpeg 编排：fetch → features 旗标 + 工具链 → 丢包重试 configure →
/// make/install。TLS 门槛是 securetransport（Apple 钥匙串，无需 CA
/// bundle）/ FFmpeg orchestration: fetch -> features flags + toolchain ->
/// the drop-and-retry configure -> make/install. The TLS gate is
/// securetransport (Apple keychain, no CA bundle needed)
pub fn ffmpeg() -> Result<(), String> {
    let e = sys::env();
    const REQUIRED: &[&str] = &[
        "libx264",
        "libx265",
        "libaom",
        "libdav1d",
        "libvpx",
        "libopus",
        "libmp3lame",
        "libfreetype",
        "libass",
        "libwebp",
        "zlib",
        "securetransport",
    ];
    let src_dir = e.work.join("ffmpeg");
    let info = crate::ffmpeg::fetch(&src_dir)?;
    fsx::write(
        &e.work.join("ffmpeg-info.txt"),
        &format!(
            "commit={}\nhash={}\nversion={}\n",
            info.commit, info.hash, info.version
        ),
    )?;
    let build = e.work.join("ffbuild-ffmpeg");
    fsx::rm_rf(&build);
    fsx::install_d(&build)?;

    let r = crate::features::resolve()?;
    if !r.enabled.iter().any(|n| n == "securetransport") {
        return Err("securetransport is not enabled; the build would have no TLS backend".into());
    }
    let mut args: Vec<String> = vec![
        "--pkg-config-flags=--static".into(),
        "--enable-gpl".into(),
        "--enable-version3".into(),
        "--disable-debug".into(),
        "--disable-autodetect".into(),
    ];
    args.extend(r.flags);
    args.extend([
        format!("--cc={}", e.cc),
        format!("--cxx={}", e.cxx),
        format!("--ar={}", e.ar),
        format!("--ranlib={}", e.ranlib),
        format!("--nm={}", e.nm),
        format!("--strip={}", e.strip),
    ]);
    // macOS 无全静态链接：依赖全走 extras 静态归档，运行时只许 OS 自带
    // 库（otool 白名单兜底）/ no full static linking on macOS: all deps
    // come in as extras' static archives and only OS-provided libraries
    // may remain dynamic (the otool allowlist catches violations)
    args.extend([
        "--extra-cflags=-DLIBTWOLAME_STATIC".into(),
        format!("--extra-libs={} -lm -liconv -lz", e.cxxrt),
        "--disable-stripping".into(),
    ]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let dropped = e.work.join("dropped-libs.txt");
    crate::ffmpeg::configure(
        &build,
        &src_dir.join("configure"),
        &refs,
        &dropped,
        REQUIRED,
    )?;
    crate::ffmpeg::build(&build, &e.work.join("ffmpeg-build.log"))
}

/// package 阶段：收集二进制/文档/man 页/预设 → otool -L 白名单（只允许
/// /usr/lib/ 与 /System/Library/）→ 冒烟测试 → BUILDINFO → tar.xz +
/// sha256（旧 macos package 移植）
/// The package stage: collect binaries/docs/man pages/presets -> the
/// `otool -L` allowlist (only /usr/lib/ and /System/Library/ may appear)
/// -> smoke test -> BUILDINFO -> tar.xz + sha256 (port of the old macOS
/// package step)
pub fn package() -> Result<(), String> {
    let e = sys::env();
    let mut hash = "unknown".to_string();
    let mut commit = "unknown".to_string();
    let mut version = "unknown".to_string();
    for l in fsx::read(&e.work.join("ffmpeg-info.txt")).lines() {
        if let Some((k, v)) = l.split_once('=') {
            match k {
                "hash" => hash = v.to_string(),
                "commit" => commit = v.to_string(),
                "version" => version = v.to_string(),
                _ => {}
            }
        }
    }
    let platform = e.plat();
    let name = format!("ffmpeg-{hash}-{platform}");
    let pkgroot = e.work.join("pkg");
    let pkg = pkgroot.join(&name);
    fsx::rm_rf(&pkgroot);
    fsx::install_d(&pkg.join("bin"))?;

    let usrlocal = Path::new(USRLOCAL);
    for exe in ["ffmpeg", "ffprobe", "ffplay"] {
        let src = usrlocal.join(format!("bin/{exe}"));
        if src.is_file() {
            fsx::install(&src, &pkg.join(format!("bin/{exe}")))?;
        }
    }
    for f in fsx::walk(&pkg.join("bin")) {
        let _ = sys::run(&e.strip, &["-x", &f.to_string_lossy()], &e.work);
    }
    let docs = usrlocal.join("share/doc/ffmpeg");
    if docs.is_dir() {
        let dst = pkg.join("doc");
        fsx::install_d(&dst)?;
        for f in fsx::walk(&docs) {
            if f.is_file() {
                let rel = f.strip_prefix(&docs).unwrap();
                fsx::install(&f, &dst.join(rel))?;
            }
        }
    }
    let man1 = usrlocal.join("share/man/man1");
    if man1.is_dir()
        && let Ok(rd) = std::fs::read_dir(&man1)
    {
        for ent in rd.flatten() {
            if ent.file_name().to_string_lossy().starts_with("ff") {
                fsx::install_d(&pkg.join("man/man1"))?;
                fsx::install(&ent.path(), &pkg.join("man/man1").join(ent.file_name()))?;
            }
        }
    }
    let presets = usrlocal.join("share/ffmpeg");
    if presets.is_dir()
        && let Ok(rd) = std::fs::read_dir(&presets)
    {
        for ent in rd.flatten() {
            if ent.file_name().to_string_lossy().ends_with(".ffpreset") {
                fsx::install_d(&pkg.join("presets"))?;
                fsx::install(&ent.path(), &pkg.join("presets").join(ent.file_name()))?;
            }
        }
    }
    let license = e.work.join("ffmpeg/COPYING.GPLv3");
    if license.is_file() {
        fsx::install(&license, &pkg.join("LICENSE.txt"))?;
    }

    // 一切必须静态链接：otool -L 只允许 OS 自带库（/usr/lib/ 与
    // /System/Library/；第一行是被查文件自身）/ everything must link
    // statically: `otool -L` may only list OS-provided libraries
    // (/usr/lib/ and /System/Library/; the first line is the inspected
    // file itself)
    log::group_begin("Static-linking check");
    let mut bad: Vec<String> = Vec::new();
    for b in fsx::walk(&pkg.join("bin")) {
        let out = sys::capture("otool", &["-L", &b.to_string_lossy()], &e.work).unwrap_or_default();
        for l in out.lines().skip(1) {
            let dep = l.split_whitespace().next().unwrap_or("");
            if dep.is_empty() {
                continue;
            }
            if !(dep.starts_with("/usr/lib/") || dep.starts_with("/System/Library/")) {
                bad.push(format!(
                    "{}:{dep}",
                    b.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    log::group_end();
    if !bad.is_empty() {
        return Err(format!(
            "binaries link non-system libraries (not portable): {}",
            bad.join(" ")
        ));
    }

    log::group_begin("Smoke test");
    let smoke_rc = crate::stages::smoke::smoke_test(&pkg.join("bin"));
    log::group_end();
    smoke_rc?;

    let buildconf = sys::capture(
        &pkg.join("bin/ffmpeg").to_string_lossy(),
        &["-hide_banner", "-buildconf"],
        &e.work,
    )
    .unwrap_or_default();
    let dropped = fsx::read(&e.work.join("dropped-libs.txt"));
    let status = fsx::read(&prefix_dir().join(".extras/status.txt"));
    let swvers = sys::capture("sw_vers", &["-productVersion"], &e.work)
        .unwrap_or_default()
        .trim()
        .to_string();
    let info = format!(
        "FFmpeg version : {version}\nFFmpeg commit  : {commit}\nPlatform       : {platform} (macOS {swvers})\nDropped by configure : {}\nExtras         :\n{}\n{buildconf}",
        dropped.replace('\n', " ").trim(),
        status
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    fsx::write(&pkg.join("BUILDINFO.txt"), &info)?;
    fsx::write(&e.dist.join(format!("{name}.buildinfo.txt")), &info)?;

    fsx::install_d(&e.dist)?;
    let tarname = format!("{name}.tar.xz");
    let tarpath = e.dist.join(&tarname);
    fsx::rm_rf(&tarpath);
    sys::bash_ok(
        &format!(
            "tar -C '{}' -cJf '{}' '{}'",
            pkgroot.to_string_lossy(),
            tarpath.to_string_lossy(),
            name
        ),
        &e.work,
    )?;
    let sum = sys::sha256_file(&tarpath)?;
    fsx::write(
        &e.dist.join(format!("{tarname}.sha256")),
        &format!("{sum}  {tarname}\n"),
    )?;
    let _ = sys::bash_ok(&format!("ls -lh '{}'", e.dist.to_string_lossy()), &e.work);
    Ok(())
}
