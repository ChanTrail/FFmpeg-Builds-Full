//! macOS arm64 平台共享助手：brew 工具链、deps 工作区快照（整目录
//! tar.zst）、FFmpeg 编排与打包。唯一入口 mac_arm64 使用。
//! Shared helpers for macOS arm64: the brew toolchain, deps workspace
//! snapshots (whole-directory tar.zst), FFmpeg orchestration and
//! packaging. The single mac_arm64 entry uses it.

use crate::fsx;
use crate::log;
use crate::sys::{self, Arch, Env, Os};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 工作根目录：$RUNNER_TEMP/ffmpeg-macos-<arch>（与日志包根目录同名，
/// runner 路径和包内路径逐字对应）/ the work root:
/// $RUNNER_TEMP/ffmpeg-macos-<arch> (same name as the log-bundle root,
/// so runner paths map to bundle paths verbatim)
pub fn work_root(arch: Arch) -> PathBuf {
    std::env::var("RUNNER_TEMP")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("ffmpeg-macos-{}", arch.name()))
}

/// 库前缀：工作区快照整树打包它（.extras/ 标记在树内）。runner 用户对
/// /opt 无写权限，所以前缀放 RUNNER_TEMP 下
/// The library prefix: deps snapshots tar this whole tree (the .extras/
/// markers ride inside). The runner user cannot write /opt, so the
/// prefix lives under RUNNER_TEMP
pub fn prefix_dir(arch: Arch) -> PathBuf {
    std::env::var("RUNNER_TEMP")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("ffmpeg-macos-{}-workspace", arch.name()))
}

/// 前缀捷径：init 之后的调用点都从全局 Env 取 arch / prefix shortcut:
/// call sites after init pull the arch from the global Env
fn prefix() -> PathBuf {
    prefix_dir(crate::sys::env().arch)
}

/// FFmpeg 默认安装前缀（configure 不传 --prefix）/ FFmpeg's default
/// install prefix (configure gets no --prefix)
pub const USRLOCAL: &str = "/usr/local";

/// 探测并安装目标架构 <arch> 的全局环境
/// probe and install the global Env for the target <arch>
pub fn init_env(arch: Arch) -> Result<(), String> {
    let bash = PathBuf::from("/bin/bash");
    if !bash.is_file() {
        return Err("/bin/bash not found".into());
    }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let prefix = prefix_dir(arch);
    let work = sys::env_dir("FFBUILD_WORK", work_root(arch));
    let dist = sys::env_dir("FFBUILD_DIST", repo_root.join("artifacts"));
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

/// packages/macos.yaml 项：每行一个 brew 包名 / packages/macos.yaml
/// entries: one brew package per line
fn package_rows() -> Result<Vec<String>, String> {
    Ok(crate::manifest::packages::load("macos.yaml")?
        .into_iter()
        .map(|p| p.name)
        .collect())
}

/// packages 阶段：brew 装 autotools 系（配方从零 autoreconf；runner 镜像
/// 不带 autotools，brew 的 libtool 是 g- 前缀的）+ `brew link --force
/// gettext`。失败不致命——镜像漂移时单包缺失不拖死构建 / the packages
/// stage: brew installs the autotools family (recipes autoreconf from
/// scratch; the runner image ships none and brew's libtool is g-prefixed),
/// then `brew link --force gettext`. Failures are non-fatal — a package
/// missing on a drifting image must not stop the build
pub fn packages() -> Result<(), String> {
    let e = sys::env();
    // work/prefix/dist 在任何子进程以它们为 cwd 前就得在场——首个
    // run() 的 current_dir 撞不存在的目录会以 "spawn prog: ENOENT" 收场
    // work/prefix/dist must exist before any child uses them as cwd —
    // the first run()'s current_dir on a missing dir dies as
    // "spawn prog: ENOENT"
    fsx::install_d(&e.work)?;
    fsx::install_d(&prefix())?;
    fsx::install_d(&e.dist)?;
    // ffmpeg 装默认前缀 /usr/local——它在 runner 上是碎片权限：
    // bin/share 可写但 lib 连创建都不行、man 子树 root 拥有。缺
    // 的 install 目标子树用 sudo 预建并还给 runner 属主；install
    // -d 对已存在目录幂等。不用 --prefix/--mandir 系重定向——保持
    // 与 linux/windows 一致的默认安装布局 / ffmpeg installs to the
    // default /usr/local prefix — a patchwork of permissions on the
    // runner: bin/share are writable but lib cannot even be created
    // and the man subtree is root-owned. The missing install-target
    // subtrees are pre-created with sudo and handed to the runner
    // owner; install -d is a no-op on existing dirs. No
    // --prefix/--mandir-style redirection — the default layout stays
    // consistent with linux/windows
    sys::bash_ok(
        "sudo -n install -d -o \"$(id -un)\" \
           /usr/local/lib /usr/local/include /usr/local/lib/pkgconfig \
           /usr/local/share/man/man1 /usr/local/share/man/man3",
        &e.work,
    )?;
    if sys::which("brew").is_none() {
        log::warn("brew not found; assuming the runner image already provides the toolchain");
        return Ok(());
    }
    log::group_begin("Install brew tools");
    let pkgs = package_rows()?;
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

/// 拉本平台 deps 产物解进 RUNNER_TEMP → .deps-sig
/// 不符时清整表完成标记（每个 .ok 自带配方哈希自会复验）
/// Pull this platform's deps asset and untar into
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
    let parent = prefix().parent().unwrap().to_path_buf();
    sys::bash_ok(
        &format!(
            "tar -C '{}' -xf '{}'",
            parent.to_string_lossy(),
            ddir.join(&restored).to_string_lossy()
        ),
        &e.work,
    )?;
    fsx::write(&e.work.join("deps-restored"), &restored)?;

    let sig_file = prefix().join(".deps-sig");
    if fsx::read(&sig_file).trim() != deps_sig() {
        log::log("deps workspace was built for different inputs; revalidating recipe markers");
        let state = prefix().join(".extras");
        let _ = std::fs::remove_file(state.join("done"));
        let _ = std::fs::remove_file(state.join("status.txt"));
        let _ = std::fs::remove_file(state.join("flags.txt"));
    }
    fsx::write(&sig_file, &format!("{}\n", deps_sig()))?;
    // 恢复的 status 是旧 run 的；save() 的 dirty 判定只看本轮真跑过的
    // 配方——与 Windows/Linux 同款无条件清。done 同样无条件清：历史
    // partial 快照可能带着"跑完但带失败"的旧标记 / the restored status
    // file is stale — save()'s dirty check only counts recipes that ran
    // this time; cleared unconditionally, same as Windows/Linux. done is
    // dropped unconditionally too: historical partial snapshots can carry
    // the old "finished with failures" marker
    let _ = std::fs::remove_file(prefix().join(".extras/status.txt"));
    let _ = std::fs::remove_file(prefix().join(".extras/done"));
    log::group_end();
    Ok(())
}

/// 工作区整树 tar -19 → deps::publish（complete = 本轮
/// 配方表零失败——done 在场 + status 非空且无 failed 行，与 Windows/
/// Whole-workspace `tar -19` ->
/// deps::publish (complete = this run's recipe table finished green —
/// done present + status non-empty with no failed lines, same predicate
/// as Windows/Linux)
pub fn save() -> Result<(), String> {
    let e = sys::env();
    let platform = e.plat().to_string();
    let ws = prefix();
    if !ws.is_dir() {
        log::log("no workspace to publish");
        return Ok(());
    }
    let restored = fsx::read(&e.work.join("deps-restored")).trim().to_string();
    // status 在 restore 已清——" ok"/" failed" 行只可能来自本轮：有进度
    // 但未全绿时 partial 快照才能发出去 / status was cleared at restore —
    // " ok"/" failed" lines can only come from this run: progress without
    // a green sweep must still publish a partial snapshot
    let status = fsx::read(&ws.join(".extras/status.txt"));
    let complete = ws.join(".extras/done").is_file()
        && !status.is_empty()
        && !status.lines().any(|l| l.ends_with(" failed"));
    let dirty = status
        .lines()
        .any(|l| l.ends_with(" ok") || l.ends_with(" failed"));
    if !restored.is_empty() && restored != format!("deps-{platform}-partial.tar.zst") && !dirty {
        log::log("deps workspace unchanged since restored snapshot; skipping pack+upload");
        return Ok(());
    }
    if !crate::deps::have_gh() {
        log::warn("GH_TOKEN/gh missing; skipping deps upload");
        return Ok(());
    }
    if sys::which("zstd").is_none() {
        log::warn("zstd missing; skipping deps publish");
        return Ok(());
    }
    let tarball = e.work.join("deps-upload.tar.zst");
    // macOS 的 tar 是 bsdtar：没有 GNU tar 的 -I/--use-compress-program，
    // 会把 'zstd -19 -T0' 当文件名去开——走管道等价物，两族 tar 通吃 /
    // macOS tar is bsdtar: no GNU -I/--use-compress-program — it opens
    // 'zstd -19 -T0' as a filename. The pipe equivalent works on both
    // tar families.
    sys::bash_ok(
        &format!(
            "tar -C '{}' -cf - '{}' | zstd -19 -T0 > '{}'",
            ws.parent().unwrap().to_string_lossy(),
            ws.file_name().unwrap().to_string_lossy(),
            tarball.to_string_lossy()
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
    // 必填库探测阶段缺席就点名早败 / fail early on required libs already
    // absent at probe time
    r.require(REQUIRED)?;
    let mut args: Vec<String> = vec![
        "--pkg-config-flags=--static".into(),
        "--enable-gpl".into(),
        "--enable-version3".into(),
        "--disable-debug".into(),
        "--disable-autodetect".into(),
    ];
    args.extend(r.flags);
    args.extend([
        // ccache 前缀由 FFBUILD_CCACHE 控制（显式调用模式，非
        // masquerade）；configure 的探针编译同样命中缓存 / the ccache
        // prefix is gated by FFBUILD_CCACHE (explicit invocation, not
        // masquerade); configure's probe compiles hit the cache too
        format!("--cc={}", sys::ccache(&e.cc)),
        format!("--cxx={}", sys::ccache(&e.cxx)),
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
        // 两类"同名同码"冲突（librav1e∩librsvg-2 的 rustc std 单态化、
        // xevd∩xeve 的 mc/recon 共享源）在 macOS 由 extras::postfix 的
        // nmedit 去重处理——ld64 已把 -multiply_defined 打成 obsolete
        // 直接忽略，ld 开关此路不通 / the two same-name same-code
        // collision classes (librav1e∩librsvg-2 rustc std monomorphs,
        // xevd∩xeve shared mc/recon sources) are handled by extras::
        // postfix's nmedit dedup on macOS — ld64 declared
        // -multiply_defined obsolete and ignores it, so no ld switch
        // exists
        // compact unwind 只编得进 3 个 personality routine；ObjC
        // (___objc_personality_v0) + C++ (___gxx_personality_v0) +
        // rsvg 的 rust_eh_personality + dedup 改名出的
        // zrust_eh_personality（rav1e 侧）= 4 个——ld64 直接拒链。
        // 关掉 compact unwind 回退 __eh_frame（DWARF）展开：功能
        // 无损、二进制还省一份 unwind 表 / compact unwind encodes
        // at most 3 personality routines; ObjC
        // (___objc_personality_v0) + C++ (___gxx_personality_v0) +
        // rsvg's rust_eh_personality + the dedup-renamed
        // zrust_eh_personality (rav1e side) = 4 — ld64 refuses to
        // link. -no_compact_unwind falls back to __eh_frame (DWARF)
        // unwinding: no functional loss and the binary even saves
        // one unwind table
        {
            // shared 模式加 @loader_path rpath：dylib 平放 bin/ 旁位
            // 解析（install_name_dir=@rpath 见下）/ shared mode adds an
            // @loader_path rpath: dylibs sit flat in bin/ and resolve
            // sibling-wise (install_name_dir=@rpath below)
            if sys::shared() {
                "--extra-ldflags=-Wl,-no_compact_unwind -Wl,-rpath,@loader_path"
            } else {
                "--extra-ldflags=-Wl,-no_compact_unwind"
            }
        }
        .into(),
        format!("--extra-libs={} -lm -liconv -lz", e.cxxrt),
    ]);
    // FFBUILD_DEBUG 非空 = 真·调试构建：--enable-debug=3 全开调试
    // 信息、--disable-optimizations 关优化、--disable-stripping 留符
    // 号表——替换头部参数里的 --disable-debug；默认发布态无这些旗标，
    // install 自动 strip（package 的 strip -x 循环在 debug 下同跳）
    // / a non-empty FFBUILD_DEBUG means a real debug build:
    // --enable-debug=3, --disable-optimizations, --disable-stripping —
    // replacing --disable-debug in the head args; the release default
    // carries none and install strips (the package strip -x loop is
    // skipped under debug too)
    if sys::debug() {
        args.retain(|a| a != "--disable-debug");
        args.extend([
            "--enable-debug=3".into(),
            "--disable-optimizations".into(),
            "--disable-stripping".into(),
        ]);
    }
    // FFBUILD_SHARED：--install-name-dir=@rpath 让 dylib 自带
    // @rpath ID（配上面的 @loader_path rpath，包内平放解析零修补）/
    // FFBUILD_SHARED: --install-name-dir=@rpath gives every dylib an
    // @rpath ID (paired with the @loader_path rpath above — flat bin/
    // resolution needs no post-build surgery)
    if sys::shared() {
        args.extend([
            "--enable-shared".into(),
            "--disable-static".into(),
            "--install-name-dir=@rpath".into(),
        ]);
    } else {
        args.push("--disable-shared".into());
    }
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
/// sha256
/// The package stage: collect binaries/docs/man pages/presets -> the
/// `otool -L` allowlist (only /usr/lib/ and /System/Library/ may appear)
/// -> smoke test -> BUILDINFO -> tar.xz + sha256
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
    // 变体后缀：shared/debug 产物与静态版同名互踩——同一驱动目录下
    // 两种形态并存，名字必须可分辨（顺序固定 -shared-debug）/
    // variant suffixes: shared/debug artifacts would shadow the
    // static one by name — both shapes can coexist under one driver
    // dir, so names must discriminate (fixed order: -shared-debug)
    let variant = format!(
        "{}{}",
        if sys::shared() { "-shared" } else { "" },
        if sys::debug() { "-debug" } else { "" },
    );
    let name = format!("ffmpeg-{hash}-{platform}{variant}");
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
    // shared 构建：lib*.*.dylib 真文件平收进 bin/（ID 已是 @rpath，
    // exe 带 @loader_path rpath，无需 install_name_tool 修补）/
    // shared build: collect the real lib*.*.dylib files flat into bin/
    // (IDs are already @rpath and the exes carry an @loader_path
    // rpath — no install_name_tool surgery needed)
    if sys::shared()
        && let Ok(rd) = std::fs::read_dir(usrlocal.join("lib"))
    {
        for ent in rd.flatten() {
            let p = ent.path();
            let n = ent.file_name().to_string_lossy().to_string();
            if n.ends_with(".dylib")
                && n.trim_end_matches(".dylib").contains('.')
                && p.is_file()
                && !p
                    .symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false)
            {
                fsx::install(&p, &pkg.join(format!("bin/{n}")))?;
            }
        }
    }
    // 打包剥符号——FFBUILD_DEBUG 下跳过（configure 那道已把
    // --disable-stripping 传给 install）/ package-time strip —
    // skipped under FFBUILD_DEBUG (the configure flag already kept
    // symbols at install)
    if !sys::debug() {
        for f in fsx::walk(&pkg.join("bin")) {
            let _ = sys::run(&e.strip, &["-x", &f.to_string_lossy()], &e.work);
        }
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
            // @rpath 依赖必须随包同行（shared 模式的 lib*.dylib）；OS
            // 自带 /usr/lib、/System/Library 照旧放行 / an @rpath dep
            // must be shipped beside the binary (shared mode's
            // lib*.dylib); OS-provided /usr/lib and /System/Library
            // stay allowed as before
            let shipped = dep
                .strip_prefix("@rpath/")
                .map(|n| pkg.join("bin").join(n).exists())
                .unwrap_or(false);
            if !(dep.starts_with("/usr/lib/") || dep.starts_with("/System/Library/") || shipped) {
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
    let status = fsx::read(&prefix().join(".extras/status.txt"));
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
