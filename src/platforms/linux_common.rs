//! Linux 平台共享助手：apt 工具链、deps 前缀快照（整前缀 tar.zst）、
//! FFmpeg 编排与打包。linux_x86_64 / linux_arm64 两个入口各自独立，
//! 共享实现在此。
//! Shared helpers for the Linux platform: the apt toolchain, deps prefix
//! snapshots (whole-prefix tar.zst), FFmpeg orchestration and packaging.
//! The linux_x86_64 / linux_arm64 entry modules stay independent but
//! share the implementation here.

use crate::fsx;
use crate::log;
use crate::sys::{self, Arch, Env, Os};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 库安装前缀：deps 快照整树打包它（含 .extras/ 标记）/ the library
/// install prefix — deps snapshots tar this whole tree (including the
/// .extras/ markers)
pub const PREFIX: &str = "/opt/ffbuild";
/// FFmpeg 默认安装前缀（configure 不传 --prefix，root 下 make install
/// 直落）/ FFmpeg's default install prefix (configure gets no --prefix;
/// `make install` lands here as root)
pub const USRLOCAL: &str = "/usr/local";

/// 探测并安装目标架构 <arch> 的全局环境
/// probe and install the global Env for the target <arch>
pub fn init_env(arch: Arch) -> Result<(), String> {
    let bash = ["/usr/bin/bash", "/bin/bash"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or("bash not found")?;
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = sys::env_dir(
        "FFBUILD_WORK",
        PathBuf::from(format!("/ffmpeg-linux-{}", arch.name())),
    );
    let dist = sys::env_dir("FFBUILD_DIST", repo_root.join("artifacts"));
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // 子进程 PATH：前缀 bin + pyenv(构建工具) + cargo(rav1e/librsvg) +
    // 系统 PATH（POSIX 冒号形式）/ child PATH: prefix bin + pyenv build
    // tools + cargo (rav1e, librsvg) + the system PATH (POSIX ':' form)
    let old_path = std::env::var("PATH").unwrap_or_default();
    let child_path = format!("{PREFIX}/bin:/opt/pyenv/bin:/opt/cargo/bin:{old_path}");
    sys::set_env(Env {
        os: Os::Linux,
        arch,
        msystem: String::new(),
        pkgprefix: String::new(),
        pacrepo: String::new(),
        msys2_root: PathBuf::from("/"),
        prefix: PREFIX.to_string(),
        env_prefix: PREFIX.to_string(),
        prefix_win: PathBuf::from(PREFIX),
        bash,
        cc: "gcc".into(),
        cxx: "g++".into(),
        ar: "gcc-ar".into(),
        ranlib: "gcc-ranlib".into(),
        nm: "gcc-nm".into(),
        strip: "strip".into(),
        objdump: "objdump".into(),
        cxxrt: "-lstdc++".into(),
        gomp: RwLock::new("-fopenmp".into()),
        jobs,
        work,
        dist,
        repo_root,
        child_path,
        inside_msys: false,
    });
    Ok(())
}

/// packages/linux.yaml 项：每行一个 apt 包名，{GCCV} 是 GCC 主版本占位符
/// packages/linux.yaml entries: one apt package per line; {GCCV} is the
/// GCC major-version placeholder
fn package_rows(gccv: u32) -> Result<Vec<String>, String> {
    Ok(crate::manifest::packages::load("linux.yaml")?
        .into_iter()
        .map(|p| p.name.replace("{GCCV}", &gccv.to_string()))
        .collect())
}

/// packages 阶段：apt 包装（整表批量 + 逐包回退）→ 最新 GCC 设为默认
/// 编译器（/usr/local/bin 遮蔽 + update-alternatives）→ pyenv 里的
/// meson/ninja/cmake → autoconf 2.73/automake 1.18 快照 .deb 直解进 /
/// → rustup + 钉版 cargo-c
/// The packages stage: apt install (one batch, per-package fallback) ->
/// newest GCC as the default compiler (/usr/local/bin shadows +
/// update-alternatives) -> meson/ninja/cmake in a pyenv -> the autoconf
/// 2.73 / automake 1.18 snapshot .debs unpacked straight into / ->
/// rustup + pinned cargo-c
pub fn packages() -> Result<(), String> {
    let e = sys::env();
    // work/前缀在任何子进程以它们为 cwd 前就得在场（runner 上整条
    // 流水线经 sudo -E 跑，这两个根属主目录建在这里比撞 current_dir
    // ENOENT 清楚）/ work and the prefix must exist before any child
    // uses them as cwd — the whole pipeline runs under `sudo -E` on
    // runners, so creating these root-owned dirs here beats a cryptic
    // current_dir ENOENT
    fsx::install_d(&e.work)?;
    fsx::install_d(Path::new(PREFIX))?;
    if sys::which("apt-get").is_none() {
        return Err("apt-get not found — the Linux pipeline targets Debian/Ubuntu".into());
    }
    log::group_begin("Install build tools");
    let _guard = sys::EnvGuard::set(&[("DEBIAN_FRONTEND", "noninteractive".to_string())]);
    sys::run_ok("apt-get", &["update"], &e.work)?;

    // 摘掉 man-db 的 auto-update 印记：它的 postinst 触发器在大批量安装
    // 时会全量重建 mandb，runner 上白烧 30-60s——构建容器不看 man 页
    // drop man-db's auto-update stamp: its postinst trigger rebuilds all of
    // mandb during bulk installs, burning 30-60s on runners — build
    // containers never read man pages
    let _ = std::fs::remove_file("/var/lib/man-db/auto-update");

    // 降序探测最新 GCC 主版本——runner 镜像还没跟上时留活路
    // probe the newest GCC major descending — leaves room on runners whose
    // image has not caught up yet
    let mut gccv = 16;
    loop {
        if sys::run("apt-cache", &["show", &format!("gcc-{gccv}")], &e.work).unwrap_or(1) == 0 {
            break;
        }
        gccv -= 1;
        if gccv < 14 {
            return Err("no gcc-14+ package offered by apt".into());
        }
    }
    log::log(&format!("gcc slot: gcc-{gccv}"));

    let pkgs = package_rows(gccv)?;
    let refs: Vec<&str> = pkgs.iter().map(String::as_str).collect();
    // 整列表装；缺包逐个回退——某个发行版没有的包不拖死整个构建
    // install the whole list; fall back one-by-one so a package missing on
    // this release never stops the build
    let mut args = vec!["install", "-y", "--no-install-recommends"];
    args.extend(&refs);
    if sys::run("apt-get", &args, &e.work)? != 0 {
        for p in &refs {
            if sys::run(
                "apt-get",
                &["install", "-y", "--no-install-recommends", p],
                &e.work,
            )? != 0
            {
                log::warn(&format!("apt package {p} could not be installed"));
            }
        }
    }

    // GCC $gccv 设为默认：先删无法解析的 alternatives 链接（arm64 镜像
    // 自带的 /etc/alternatives/cpp 环回链接会让 update-alternatives 以
    // "Too many levels of symbolic links" 中止），再用 /usr/local/bin
    // 遮蔽（PATH 优先于 /usr/bin）；update-alternatives 失败绝不让构建
    // 死——光靠遮蔽就已经提供了正确编译器
    // Make GCC $gccv the default: drop unresolvable alternatives links
    // first (the arm64 image ships an /etc/alternatives/cpp symlink that
    // loops back and aborts update-alternatives with "Too many levels of
    // symbolic links"), then shadow via /usr/local/bin (earlier in PATH
    // than /usr/bin); never let update-alternatives kill the build — the
    // shadows alone already provide the right compiler
    for tool in ["gcc", "g++", "gcc-ar", "gcc-ranlib", "gcc-nm", "cpp"] {
        let alt = PathBuf::from(format!("/etc/alternatives/{tool}"));
        if alt
            .symlink_metadata()
            .map(|m| m.is_symlink())
            .unwrap_or(false)
            && !alt.exists()
        {
            let _ = std::fs::remove_file(&alt);
        }
        let versioned = PathBuf::from(format!("/usr/bin/{tool}-{gccv}"));
        if versioned.is_file() {
            let shadow = PathBuf::from(format!("/usr/local/bin/{tool}"));
            let _ = std::fs::remove_file(&shadow);
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&versioned, &shadow);
            // Windows 交叉编译时的兜底：ln -sf 等价物（本模块在 Windows
            // 上只走编译不执行）/ fallback for cross-compiles from Windows
            // (this module compiles on Windows but never executes there)
            #[cfg(windows)]
            let _ = std::fs::copy(&versioned, &shadow);
            let prio = (gccv * 10).to_string();
            let _ = sys::run(
                "update-alternatives",
                &[
                    "--install",
                    &format!("/usr/bin/{tool}"),
                    tool,
                    &format!("/usr/bin/{tool}-{gccv}"),
                    &prio,
                ],
                &e.work,
            );
        }
    }
    let _ = sys::bash("hash -r; gcc --version | head -n1", &e.work);

    // meson/ninja/cmake 装进 virtualenv（系统 Python 是 "externally
    // managed"）；/opt/pyenv 随 deps 快照走：二进制在场即跳过
    // meson, ninja, cmake & co. live in a virtualenv (the system Python is
    // externally managed); /opt/pyenv rides the deps snapshot: present
    // binaries skip creation + pip
    if !(Path::new("/opt/pyenv/bin/meson").is_file()
        && Path::new("/opt/pyenv/bin/ninja").is_file()
        && Path::new("/opt/pyenv/bin/cmake").is_file())
    {
        sys::run_ok("python3", &["-m", "venv", "/opt/pyenv"], &e.work)?;
        sys::run_ok(
            "/opt/pyenv/bin/pip",
            &[
                "install",
                "--quiet",
                "meson",
                "ninja",
                "cmake",
                "jinja2",
                "jsonschema",
                "setuptools",
            ],
            &e.work,
        )?;
    } else {
        log::log("python build-tool venv restored from cache");
    }

    // 把 .deb 的 payload 直解进 /——不用 dpkg -i：它的 maintainer 脚本
    // 会调 update-alternatives，在 noble runner 上撞上预先坏掉的
    // /usr/bin/cpp 链接而死（Ubuntu bug #2067157），还会经 apt-get -f
    // 把 libltdl-dev 卸了。lame 要 AC_PREREQ(2.73)（已发布 tarball 没有），
    // libiconv 的 autogen 专门要 aclocal-1.18
    // Unpack .deb payloads straight into / — NOT dpkg -i: the maintainer
    // scripts call update-alternatives which dies on the noble runners'
    // pre-broken /usr/bin/cpp link (Ubuntu bug #2067157) and, via apt-get
    // -f, even removed libltdl-dev. lame demands AC_PREREQ(2.73) which no
    // released tarball provides; libiconv's autogen wants aclocal-1.18
    // specifically
    for (name, url) in [
        (
            "autoconf",
            "https://archive.ubuntu.com/ubuntu/pool/main/a/autoconf/autoconf_2.73-2_all.deb",
        ),
        (
            "automake",
            "https://archive.ubuntu.com/ubuntu/pool/main/a/automake/automake_1.18.1-4_all.deb",
        ),
    ] {
        let deb = e.work.join(format!("{name}.deb"));
        crate::net::download_sync(url, &deb)?;
        sys::run_ok("dpkg", &["-x", &deb.to_string_lossy(), "/"], &e.work)?;
        let _ = std::fs::remove_file(&deb);
    }
    if sys::bash("command -v aclocal-1.18 automake-1.18 >/dev/null", &e.work)? != 0 {
        return Err("automake 1.18 payload missing after extraction".into());
    }

    // Rust（rav1e、librsvg……）+ 钉版 cargo-c（librsvg 2.63.x 要
    // >= 0.10.10；latest/download 随上游漂移破坏可复现性）
    // Rust (rav1e, librsvg, ...) + pinned cargo-c (librsvg 2.63.x wants
    // >= 0.10.10; the latest/download endpoint drifts upstream and breaks
    // reproducibility)
    fsx::install_d(Path::new("/opt/cargo/bin"))?;
    if !(Path::new("/opt/cargo/bin/cargo").is_file() && Path::new("/opt/cargo/bin/rustc").is_file())
    {
        sys::bash_ok(
            "curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable",
            &e.work,
        )?;
    } else {
        log::log("rust toolchain restored from cache");
    }
    if !Path::new("/opt/cargo/bin/cargo-cbuild").is_file() {
        let cc_arch = match e.arch {
            Arch::Arm64 => "aarch64",
            _ => "x86_64",
        };
        sys::bash_ok(
            &format!(
                "curl -fsSL https://github.com/lu-zero/cargo-c/releases/download/v0.10.26/cargo-c-{cc_arch}-unknown-linux-musl.tar.gz | tar -xz -C /opt/cargo/bin"
            ),
            &e.work,
        )?;
    } else {
        log::log("cargo-c restored from cache");
    }
    log::group_end();
    Ok(())
}

/// deps 签名：配方表 + sources/features 清单 + 工具链指纹 —— 恢复的
/// 前缀若由不同输入建的就要复验标记 / the deps signature: recipe table +
/// sources/features manifests + the toolchain fingerprint — a prefix
/// restored from different inputs gets its markers revalidated
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

/// extras 阶段入口：恢复 deps 前缀 → 按签名复验标记 → 跑配方表。
/// Linux 的前缀快照是整树 tar.zst（.extras/ 标记在树内随之往返）——
/// 比 Windows 的差集 manifest 模型简单。
/// The extras stage entry: restore the deps prefix -> revalidate markers
/// against the signature -> run the recipe table. On Linux the prefix
/// snapshot is a whole-tree tar.zst (the .extras/ markers ride inside
/// it) — simpler than Windows' delta-manifest model.
pub fn extras_stage() -> Result<(), String> {
    restore()?;
    crate::extras::run_all()
}

/// 拉本平台的 deps 产物解进 / → .deps-sig 不符时逐
/// 个复验 .extras/*.ok（每个标记自带配方哈希，重哈希不符即删——单个
/// 配方改动只重建它自己而不是全部）→ 清 done/status 让整表重走
/// Pull this platform's deps asset and untar into /
/// -> on a .deps-sig mismatch revalidate every .extras/*.ok (each marker
/// stores its recipe hash; a mismatch drops just that recipe, not all
/// 60+) -> drop done/status so the table rewalks
pub fn restore() -> Result<(), String> {
    let e = sys::env();
    let ddir = e.work.join("deps-dl");
    fsx::install_d(&ddir)?;
    let restored = match crate::deps::fetch(&e.plat(), &ddir) {
        Ok(a) => a,
        Err(_) => {
            log::log("no deps snapshot available; building the prefix from scratch");
            return Ok(());
        }
    };
    log::group_begin(&format!("Restore deps prefix ({restored})"));
    sys::bash_ok(
        &format!(
            "tar -C / --keep-old-files -xf '{}' || tar -C / -xf '{}'",
            ddir.join(&restored).to_string_lossy(),
            ddir.join(&restored).to_string_lossy()
        ),
        &e.work,
    )?;
    fsx::write(&e.work.join("deps-restored"), &restored)?;

    let sig_file = Path::new(PREFIX).join(".deps-sig");
    if fsx::read(&sig_file).trim() != deps_sig() {
        log::log("deps prefix was built for different inputs; revalidating recipe markers");
        // .extras/*.ok 各自存配方哈希：extras_run 自会重建实际变了的
        // 配方，这里只需清掉整表完成标记和旧状态行
        // each .extras/*.ok stores its recipe hash: extras_run rebuilds
        // only the recipes that actually changed — here we just drop the
        // whole-table done marker and the stale status lines
        let state = Path::new(PREFIX).join(".extras");
        let _ = std::fs::remove_file(state.join("done"));
        let _ = std::fs::remove_file(state.join("status.txt"));
        let _ = std::fs::remove_file(state.join("flags.txt"));
    }
    fsx::write(&sig_file, &format!("{}\n", deps_sig()))?;
    // 恢复的 status 是旧 run 的；save() 的 dirty 判定只看本轮真跑过的
    // 配方——与 Windows 同款无条件清。done 同样无条件清：历史 partial
    // 快照里可能带着它——不清掉的话 extras 没跑的 run 会把这个陈旧
    // 标记当成"完成"发出去
    // the restored status file is stale — save()'s dirty check only
    // counts recipes that ran this time; cleared unconditionally, same
    // as Windows. done is dropped unconditionally too: historical partial
    // snapshots may carry it — left in place, a run where extras never ran
    // would treat the stale marker as "complete"
    let _ = std::fs::remove_file(Path::new(PREFIX).join(".extras/status.txt"));
    let _ = std::fs::remove_file(Path::new(PREFIX).join(".extras/done"));
    log::group_end();
    Ok(())
}

/// 整前缀 tar -19 → deps::publish。complete = 本轮配方表
/// 零失败收尾（done 在场 + status 非空且无 failed 行——与 Windows 同款
/// 判定；单看 done 会被历史快照里"跑完但带失败"的标记骗到）/
/// Whole-prefix `tar -19` -> deps::publish. complete = this
/// run's recipe table finished green (done present + status non-empty
/// with no failed lines — same predicate as Windows; done alone is
/// fooled by "finished with failures" markers inside old snapshots)
pub fn save() -> Result<(), String> {
    let e = sys::env();
    let platform = e.plat().to_string();
    let restored = fsx::read(&e.work.join("deps-restored")).trim().to_string();
    // status 在 restore 已清——" ok"/" failed" 行只可能来自本轮：有进度
    // 但未全绿时 partial 快照才能发出去（此前 dirty=false 恒真跳过，
    // Linux 永远发不出续跑点）/ status was cleared at restore — " ok"/
    // " failed" lines can only come from this run: progress without a
    // green sweep must still publish a partial snapshot (the old
    // hardcoded dirty=false meant Linux could never ship a resume point)
    let status = fsx::read(&Path::new(PREFIX).join(".extras/status.txt"));
    let complete = Path::new(PREFIX).join(".extras/done").is_file()
        && !status.is_empty()
        && !status.lines().any(|l| l.ends_with(" failed"));
    let dirty = status
        .lines()
        .any(|l| l.ends_with(" ok") || l.ends_with(" failed"));
    if !restored.is_empty() && restored != format!("deps-{platform}-partial.tar.zst") && !dirty {
        log::log("deps prefix unchanged since restored snapshot; skipping pack+upload");
        return Ok(());
    }
    if !crate::deps::have_gh() {
        log::warn("GH_TOKEN/gh missing; skipping deps upload");
        return Ok(());
    }
    let tarball = e.work.join("deps-upload.tar.zst");
    sys::bash_ok(
        &format!(
            "tar -C / -I 'zstd -19 -T0' -cf '{}' opt/ffbuild",
            tarball.to_string_lossy()
        ),
        &e.work,
    )?;
    crate::deps::publish(&platform, &restored, &tarball, complete, dirty)?;
    let _ = std::fs::remove_file(&tarball);
    Ok(())
}

/// FFmpeg 编排：fetch → features 旗标 + 工具链 → 丢包重试 configure →
/// make/install。Linux 不传 --prefix——以 root 跑时 make install 直落
/// /usr/local，与打包收集路径一致
/// FFmpeg orchestration: fetch -> features flags + toolchain -> the
/// drop-and-retry configure -> make/install. No --prefix on Linux —
/// running as root, `make install` lands in /usr/local which is where
/// the package stage collects from
/// 逐 token 重写 .pc 文本里的运行时库 -l 名。返回是否改动 /
/// Rewrite runtime -l names in .pc text token by token; returns whether
/// anything changed
fn sanitize_pc_text(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut tok = String::new();
    let mut changed = false;
    let flush = |tok: &mut String, out: &mut String, changed: &mut bool| {
        if let Some(orig) = tok.strip_prefix("-l") {
            // -l<name>_asneeded → -l<name>：Ubuntu/Debian gcc 的 as-needed
            // 包装名只有 .so/链接脚本无 .a（libgcc_s_asneeded、
            // libatomic_asneeded 等）；gcc_s_asneeded 先折叠出 gcc_s
            // 再进下一条规则 / -l<name>_asneeded → -l<name>: Ubuntu/Debian
            // gcc's as-needed wrapper names exist only as .so/linker
            // scripts, never as .a (libgcc_s_asneeded, libatomic_asneeded
            // ...); gcc_s_asneeded collapses to gcc_s first, then the rule
            // below finishes it
            let name = orig.strip_suffix("_asneeded").unwrap_or(orig);
            // -lgcc_s → -lgcc：共享版 libgcc 同样只有 .so，全静态下必须
            // 落到 libgcc.a / -lgcc_s → -lgcc: the shared libgcc name
            // resolves only to .so; under -static it must fall back to
            // libgcc.a
            let name = if name == "gcc_s" { "gcc" } else { name };
            *changed |= name != orig;
            out.push_str("-l");
            out.push_str(name);
        } else {
            out.push_str(tok);
        }
        tok.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            flush(&mut tok, &mut out, &mut changed);
            out.push(ch);
        } else {
            tok.push(ch);
        }
    }
    flush(&mut tok, &mut out, &mut changed);
    (out, changed)
}

/// 全静态化前的 pkg-config 消毒：配方产出的 .pc 其 Libs.private 带着发行
/// 版工具链的运行时库共享名（rav1e/x265/srt/librsvg 的 -lgcc_s，whisper
/// 的 -lgcc_s_asneeded/-latomic_asneeded）——--pkg-config-flags=--static
/// 会把它们原样吐进 configure 探测链接行，而 -static 下根本没有
/// libgcc_s.a/libatomic_asneeded.a。Libs.private 只在 --static 查询展开，
/// shared 变体改了也无伤 / pkg-config sanitization ahead of full-static:
/// recipe-produced .pc Libs.private carry distro-toolchain shared runtime
/// names (-lgcc_s from rav1e/x265/srt/librsvg, -lgcc_s_asneeded /
/// -latomic_asneeded from whisper) — --pkg-config-flags=--static expands
/// them verbatim into configure probe link lines, and no libgcc_s.a /
/// libatomic_asneeded.a exists under -static. Libs.private only expands
/// for --static queries, so the rewrite is harmless for the shared variant
fn sanitize_pc() -> Result<(), String> {
    let mut n = 0usize;
    for sub in ["lib/pkgconfig", "share/pkgconfig"] {
        let dir = Path::new(PREFIX).join(sub);
        for f in fsx::walk(&dir) {
            if f.extension().and_then(|x| x.to_str()) != Some("pc") {
                continue;
            }
            let text = fsx::read(&f);
            let (fixed, changed) = sanitize_pc_text(&text);
            if changed {
                fsx::write(&f, &fixed)?;
                n += 1;
            }
        }
    }
    if n > 0 {
        println!("[pkgconfig] sanitized runtime -l names in {n} .pc file(s)");
    }
    Ok(())
}

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
        "gnutls",
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
    if !r.enabled.iter().any(|n| n == "gnutls") {
        return Err("gnutls is not enabled; the build would have no TLS backend".into());
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
    // 整个二进制 -static 全静态（旧顾虑——getpwnam/iconv/NSS 全靠
    // dlopen——已不成立：iconv 是自编 libiconv，getaddrinfo 有内置
    // files/dns，dlopen 在 glibc≥2.34 的静态二进制里合法）；C++ 运行
    // 时的兜底仍是 -lstdc++ 尾巴（见下）/ the whole binary goes
    // -static (the old objection — getpwnam/iconv/NSS dlopening — no
    // longer applies: iconv is our own libiconv, getaddrinfo has
    // built-in files/dns, and dlopen is legal in static binaries on
    // glibc≥2.34); the C++ runtime still rides the -lstdc++ tail below
    let gomp = e.gomp.read().unwrap().clone();
    args.extend([
        "--extra-cflags=-DLIBTWOLAME_STATIC".into(),
        // xevd/xeve 同族源码撞名（padding/prefetch_for_mc 等共享 mc/recon
        // 非 static 符号），静态归档同链必 multiple definition——windows
        // 在 fixup 改名去重，这里用 GNU ld 的宽容开关兜底 / xevd and
        // xeve share source files whose non-static symbols collide
        // (padding, prefetch_for_mc, ... mc/recon helpers) — statically
        // archiving both into one link is a guaranteed multiple
        // definition; windows dedupes by renaming in fixup, here GNU
        // ld's tolerant switch is the umbrella
        // -static 全静态：连 glibc 家族也打进二进制（-static-libgcc/
        // -static-libstdc++ 是 -static 的子集，删掉）。前缀库全是 .a，
        // 剩余 -l 只剩 libc/libm/libpthread/libdl/librt/libresolv/
        // libatomic/libgcc——glibc dev 与 gcc 都带 .a。运行时行为差异：
        // getaddrinfo 回落内置 files/dns NSS（正常 DNS 不受影响，mDNS
        // 这类插件式解析器失效）；dlopen 在 glibc≥2.34 的静态二进制里
        // 照常工作（vaapi/vdpau/nvenc/opencl/vulkan 驱动按需载入不受
        // 影响）；iconv 是自编 libiconv 不走 gconv——三大静态化雷区都
        // 不涉及 / -static all the way: even the glibc family goes into
        // the binary (-static-libgcc/-static-libstdc++ are subsets of
        // -static, dropped). Every prefix lib is a .a already; the only
        // -l's left are libc/libm/libpthread/libdl/librt/libresolv/
        // libatomic/libgcc — all shipped as .a by libc dev + gcc.
        // Runtime deltas: getaddrinfo falls back to the built-in
        // files/dns NSS (normal DNS unaffected, plugin resolvers like
        // mDNS won't load); dlopen keeps working in static binaries on
        // glibc≥2.34 (vaapi/vdpau/nvenc/opencl/vulkan driver loading is
        // untouched); iconv is our own libiconv, not gconv — the three
        // classic static-linking pitfalls are all non-issues
        {
            // shared 模式 -static 与 --enable-shared 互斥（.so 无法
            // -static）——只摘全 -static，保留 gcc/C++ 运行时静态化 /
            // -static and --enable-shared are mutually exclusive
            // (shared objects can't be -static) — drop only the blanket
            // -static and keep the gcc/C++ runtimes static
            if sys::shared() {
                "--extra-ldflags=-static-libgcc -static-libstdc++ -Wl,--allow-multiple-definition"
            } else {
                "--extra-ldflags=-static -Wl,--allow-multiple-definition"
            }
        }
        .into(),
        // -lstdc++ 放 extra-libs 尾部：check_lib 型链测（openh264 除外
        // 全走 .pc）不查 .pc，C++ 归档的 operator delete/vtable 引用只有
        // 全局尾巴能兜底——openal/裸 -l 检查全靠它（实证：libjxl 修复后
        // configure 走通，才暴露出 openal -lopenal 检查缺 C++ 运行时）。
        // -lm 必须比 -lstdc++ 更靠后：GCC≥13 的 libstdc++.a 里
        // __floating_to_chars 引用 fegetround/fesetround，静态归档按序
        // 扫描不回拉——-lm 在前则 openal 探测直接 undefined / -lstdc++
        // sits in extra-libs' tail: check_lib-style link probes (every
        // pkg-config check except openh264) never consult a .pc, and a C++
        // archive's operator delete/vtable references only a global tail
        // can satisfy — the bare -lopenal check depends on it entirely
        // (empirical: once libjxl stopped aborting configure, the -lopenal
        // check surfaced missing its C++ runtime). -lm must trail -lstdc++:
        // GCC≥13's libstdc++.a __floating_to_chars references
        // fegetround/fesetround, and static archives scan in order without
        // backtracking — -lm earlier leaves the openal probe undefined
        format!("--extra-libs={gomp} -lpthread -lz -lstdc++ -lm"),
    ]);
    // FFBUILD_DEBUG 非空 = 真·调试构建：--enable-debug=3 全开调试
    // 信息、--disable-optimizations 关掉全部优化、--disable-stripping
    // 保留符号表——替换掉头部参数里的 --disable-debug；默认发布态无
    // 这些旗标，install 自动 strip / a non-empty FFBUILD_DEBUG means
    // a real debug build: --enable-debug=3 for full debug info,
    // --disable-optimizations to kill all optimizations, and
    // --disable-stripping to keep the symbol table — replacing the
    // --disable-debug in the head args; the release default carries
    // none of them and install strips automatically
    if sys::debug() {
        args.retain(|a| a != "--disable-debug");
        args.extend([
            "--enable-debug=3".into(),
            "--disable-optimizations".into(),
            "--disable-stripping".into(),
        ]);
    }
    // FFBUILD_SHARED：--enable-rpath 把 install 树的 libdir 烙进
    // 程序（usrlocal/bin 即装即用）；package 再把包内副本重打成
    // $ORIGIN（flat bin/ 布局见 package）/ FFBUILD_SHARED:
    // --enable-rpath bakes the install tree's libdir into the programs
    // (usrlocal/bin works in place); package() then rewrites the
    // shipped copies to $ORIGIN for the flat bin/ layout
    if sys::shared() {
        args.extend([
            "--enable-shared".into(),
            "--disable-static".into(),
            "--enable-rpath".into(),
        ]);
    } else {
        args.push("--disable-shared".into());
    }
    // .pc 静态名消毒必须在 configure 探测之前（探测链接行直接消费
    // Libs.private）/ the .pc static-name sanitization must run before
    // configure probing (probe link lines consume Libs.private verbatim)
    sanitize_pc()?;
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

/// package 阶段：收集二进制/文档/man 页/预设 → ldd 白名单（仅 glibc
/// 家族允许动态依赖）→ 冒烟测试 → BUILDINFO → tar.xz + sha256
/// The package stage: collect binaries/docs/man pages/presets -> the ldd
/// allowlist (only the glibc family may appear as dynamic deps) -> smoke
/// test -> BUILDINFO -> tar.xz + sha256
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
    // shared 构建：lib*.so.N 真文件平收进 bin/（前缀里只有 ffmpeg
    // 自己的 .so——依赖全是 .a）；收完给整层打 $ORIGIN rpath：
    // exe 与互引的 .so 一律同目录解析，包拎到任何路径即跑 /
    // shared build: collect the real lib*.so.N files flat into bin/
    // (the prefix only has ffmpeg's own .so — deps are all .a), then
    // stamp $ORIGIN on the whole bin layer: exes and their mutual .so
    // references all resolve sibling-wise, so the package runs from
    // any path
    if sys::shared() {
        if let Ok(rd) = std::fs::read_dir(usrlocal.join("lib")) {
            for ent in rd.flatten() {
                let p = ent.path();
                let n = ent.file_name().to_string_lossy().to_string();
                if n.contains(".so.")
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
        // SONAME 链接别名收第二趟：exe/.so 的 NEEDED 记的是短名
        // （libavcodec.so.63——真文件是 .so.63.28.100 全版本），只收
        // 真文件 ld.so 按名找不到、ldd 白名单同判；别名以实体副本入
        // 包（fs::copy 随链接取目标字节），不带版本段的 .so dev 链接
        // 跳过 / second pass for the SONAME aliases: the exes'/.sos'
        // NEEDED entries record the short name (libavcodec.so.63 —
        // the real file is the fully-versioned .so.63.28.100), so
        // real files alone leave ld.so — and the ldd allowlist —
        // resolving a name that isn't there. Aliases ship as real
        // copies (fs::copy follows the link for identical bytes);
        // the unversioned .so dev links are skipped
        if let Ok(rd) = std::fs::read_dir(usrlocal.join("lib")) {
            for ent in rd.flatten() {
                let p = ent.path();
                let n = ent.file_name().to_string_lossy().to_string();
                if n.contains(".so.")
                    && p.symlink_metadata()
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false)
                {
                    fsx::install(&p, &pkg.join(format!("bin/{n}")))?;
                }
            }
        }
        for f in fsx::walk(&pkg.join("bin")) {
            sys::run(
                "patchelf",
                &["--set-rpath", "$ORIGIN", &f.to_string_lossy()],
                &e.work,
            )?;
        }
    }
    // 打包剥符号——FFBUILD_DEBUG 下跳过（configure 那道已把
    // --disable-stripping 传给 install，这层再剥就白留了）/
    // package-time strip — skipped under FFBUILD_DEBUG (the configure
    // flag already kept symbols at install; this pass would undo it)
    if !sys::debug() {
        for f in fsx::walk(&pkg.join("bin")) {
            let _ = sys::run(
                &e.strip,
                &["--strip-unneeded", &f.to_string_lossy()],
                &e.work,
            );
        }
    }
    for (src, dst) in [
        (usrlocal.join("share/doc/ffmpeg"), "doc"),
        (usrlocal.join("share/man"), "man"),
    ] {
        if src.is_dir() {
            let dst = pkg.join(dst);
            fsx::install_d(&dst)?;
            for f in fsx::walk(&src) {
                if f.is_file() {
                    let rel = f.strip_prefix(&src).unwrap();
                    fsx::install(&f, &dst.join(rel))?;
                }
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
        fsx::install(&license, &pkg.join("LICENSE"))?;
    }

    // 运行时依赖白名单：默认全静态产物的 ldd 输出是 "not a dynamic
    // executable"（首词 "not" 在白名单里天然放行）；shared 变体才真
    // 有动态依赖——glibc 家族照旧放行 + 随包同行的 lib*.so.N（上方
    // shipped 判断）。libmvec.so.1 被 Ubuntu 的 libm.so 链接脚本无条
    // 件拖入（glibc 2.35 起的 GROUP），libgcc_s 是编译器运行时 /
    // runtime-dependency allowlist: the default all-static binary's
    // ldd says "not a dynamic executable" (the "not" token is already
    // allowlisted); only the shared variant has real dynamic deps —
    // the glibc family stays allowed plus sonames shipped in the
    // package (the shipped check above). libmvec.so.1 is dragged in by
    // Ubuntu's libm.so linker script (a GROUP() since glibc 2.35) and
    // libgcc_s is the compiler runtime
    log::group_begin("Dynamic dependencies");
    let allowed = [
        "linux-vdso",
        "/lib64/ld-linux",
        "/lib/ld-linux",
        "ld-linux",
        "libc.so",
        "libm.so",
        "libmvec.so",
        "libgcc_s.so",
        "libdl.so",
        "libpthread.so",
        "librt.so",
        "libutil.so",
        "libresolv.so",
        "libnsl.so",
        "not",
    ];
    let mut bad: Vec<String> = Vec::new();
    for b in fsx::walk(&pkg.join("bin")) {
        log::log(&format!("== ldd {}", b.display()));
        let out = sys::capture("ldd", &[&b.to_string_lossy()], &e.work).unwrap_or_default();
        for l in out.lines() {
            let dep = l.split_whitespace().next().unwrap_or("");
            if dep.is_empty() {
                continue;
            }
            // 随包同行的 soname（shared 模式的 lib*.so.N）合法 /
            // a soname shipped beside the binary is fine (shared mode's
            // lib*.so.N)
            if allowed.iter().any(|a| dep.starts_with(a)) || pkg.join("bin").join(dep).exists() {
                continue;
            }
            bad.push(format!(
                "{}:{dep}",
                b.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    log::group_end();
    if !bad.is_empty() {
        return Err(format!(
            "binaries depend on non-glibc shared libraries (not portable): {}",
            bad.join(" ")
        ));
    }

    log::group_begin("Smoke test");
    let smoke_rc = crate::stages::smoke::smoke_test(&pkg.join("bin"));
    log::group_end();
    smoke_rc?;

    // BUILDINFO 收集 + tar.xz 打包 + sha256 校验和
    // BUILDINFO + tar.xz + sha256
    let buildconf = sys::capture(
        &pkg.join("bin/ffmpeg").to_string_lossy(),
        &["-hide_banner", "-buildconf"],
        &e.work,
    )
    .unwrap_or_default();
    let dropped = fsx::read(&e.work.join("dropped-libs.txt"));
    let status = fsx::read(&Path::new(PREFIX).join(".extras/status.txt"));
    let ccid = sys::capture(&e.cc, &["-dumpmachine"], &e.work)
        .unwrap_or_default()
        .trim()
        .to_string();
    let libc = sys::capture("ldd", &["--version"], &e.work)
        .unwrap_or_default()
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().last().map(|s| s.to_string()))
        .unwrap_or_default();
    // BUILDINFO.info 版式向发行样例靠齐：品牌头（变体词随产物）→
    // Version/License/Source Code → External Assets → "<变体> build
    // configuration:" 领起 -buildconf 原文；dropped/extras 附尾 /
    // the BUILDINFO.info layout follows the release sample: a branded
    // header (variant word follows the artifact) ->
    // Version/License/Source Code -> External Assets -> "<variant>
    // build configuration:" introducing the verbatim -buildconf dump;
    // dropped/extras trail at the end
    let vword = if variant.is_empty() {
        "static"
    } else {
        variant.trim_start_matches('-')
    };
    // External Assets 按本二进制 -buildconf 里的实际启用旗标生成——
    // 需要运行时 dlopen 插件/数据文件的特性才有资产可列（windows-
    // arm64 的 tesseract 缺席自动不列）/ the External Assets section
    // is generated from the flags actually enabled in this binary's
    // -buildconf — only features needing runtime-dlopen plugins or
    // data files get an asset entry (windows-arm64's absent tesseract
    // drops out by itself)
    let assets = crate::ffmpeg::external_assets(&buildconf);
    let info = format!(
        "FFmpeg 64-bit {vword} Linux build from www.chantrail.com\n\
        \n\
        Version: {version}\n\
        \n\
        License: GPL v3\n\
        \n\
        Source Code: https://git.ffmpeg.org/gitweb/ffmpeg.git/commit/{commit}\n\
        \n\
        {assets}\
        {vword} build configuration: \n\
        \n\
        {buildconf}\n\
        Platform             : {platform} ({ccid}, glibc {libc})\n\
        Dropped by configure : {}\n\
        Extras               :\n{}\n",
        dropped.replace('\n', " ").trim(),
        status
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    fsx::write(&pkg.join("BUILDINFO.info"), &info)?;

    fsx::install_d(&e.dist)?;
    let tarname = format!("{name}.tar.xz");
    let tarpath = e.dist.join(&tarname);
    fsx::rm_rf(&tarpath);
    sys::bash_ok(
        &format!(
            "tar -C '{}' --owner=0 --group=0 --numeric-owner -I 'xz -T0' -cf '{}' '{}'",
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
