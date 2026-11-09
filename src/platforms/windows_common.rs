//! Windows 平台共享助手：MSYS2 环境探测、pacman 包栈、deps 覆盖层。
//! 两个 windows_*_* 入口模块各自独立，但 msys2 探测逻辑共享在此。
//! Shared helpers for the Windows platform: MSYS2 env probing, the pacman
//! package stack and deps overlays. Both windows_*_* entry modules stay
//! independent but share the msys2 probing logic here.

use crate::fsx;
use crate::log;
use crate::sys::{self, Arch, Env, Os};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::RwLock;

/// MSYSTEM/工具链参数：x86_64→UCRT64+gcc，arm64→CLANGARM64+clang
/// MSYSTEM/toolchain parameters: x86_64→UCRT64+gcc, arm64→CLANGARM64+clang
pub struct WinProfile {
    /// MSYSTEM 名 / MSYSTEM name
    pub msystem: &'static str,
    /// pacman 包前缀 / pacman package prefix
    pub pkgprefix: &'static str,
    /// pacman 仓库名 / pacman repo name
    pub pacrepo: &'static str,
    /// 前缀目录名（msys2 根下）/ prefix dir under the msys2 root
    pub envdir: &'static str,
    /// 编译器族 / compiler family
    pub clang: bool,
}

/// 按架构取参数 / pick the profile by arch
pub fn profile(arch: Arch) -> WinProfile {
    match arch {
        Arch::Arm64 => WinProfile {
            msystem: "CLANGARM64",
            pkgprefix: "mingw-w64-clang-aarch64",
            pacrepo: "clangarm64",
            envdir: "clangarm64",
            clang: true,
        },
        _ => WinProfile {
            msystem: "UCRT64",
            pkgprefix: "mingw-w64-ucrt-x86_64",
            pacrepo: "ucrt64",
            envdir: "ucrt64",
            clang: false,
        },
    }
}

/// 定位 MSYS2 根：FFBUILD_MSYS2 > 常见安装位 > bash.exe 所在
/// Locate the MSYS2 root: FFBUILD_MSYS2 > well-known install spots > the
/// dir above bash.exe
fn msys2_root() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("FFBUILD_MSYS2") {
        let p = PathBuf::from(p);
        if p.join("usr/bin/bash.exe").is_file() {
            return Ok(p);
        }
    }
    // 已在 msys2 shell 内：MSYS2_ROOT 或 /usr/bin 反推 / inside msys2
    // already: MSYS2_ROOT or infer from /usr/bin
    if let Ok(p) = std::env::var("MSYS2_ROOT") {
        let p = PathBuf::from(p);
        if p.join("usr/bin/bash.exe").is_file() {
            return Ok(p);
        }
    }
    for c in [r"D:\tools\msys64", r"C:\msys64", r"C:\tools\msys64"] {
        let p = PathBuf::from(c);
        if p.join("usr/bin/bash.exe").is_file() {
            return Ok(p);
        }
    }
    Err("MSYS2 root not found (set FFBUILD_MSYS2)".to_string())
}

/// 探测并安装目标架构 <arch> 的全局环境
/// probe and install the global Env for the target <arch>
pub fn init_env(arch: Arch) -> Result<(), String> {
    let pr = profile(arch);
    let root = msys2_root()?;
    let prefix_win = root.join(pr.envdir);
    let bash = root.join("usr/bin/bash.exe");
    if !bash.is_file() {
        return Err(format!("{} missing", bash.display()));
    }
    let prefix = format!(
        "{}/{}",
        root.to_string_lossy().replace('\\', "/"),
        pr.envdir
    );
    let (cc, cxx, cxxrt) = if pr.clang {
        ("clang", "clang++", "-lc++")
    } else {
        ("gcc", "g++", "-lstdc++")
    };
    // 子进程 PATH：前缀 bin 优先，其次是 msys2 /usr/bin（Windows 分号形
    // 式，Command 直接用）/ child PATH: prefix bin first, then msys2
    // /usr/bin — Windows ';' form, used verbatim by Command
    let mut child_path = format!(
        "{};{}",
        prefix_win.join("bin").to_string_lossy(),
        root.join("usr/bin").to_string_lossy()
    );
    // gh 只住原生 Windows 侧（GitHub CLI 目录），child_path 刻意收窄后
    // 它够不着——从进程 PATH 把含 gh.exe 的那一格并进来：该目录只有
    // gh.exe 一个可执行文件，泄漏面最小；找不到则 deps 发布照旧按
    // have_gh 空操作处理 / gh lives only on the native Windows side
    // (the GitHub CLI dir) which the deliberately narrow child_path
    // cannot reach — fold the one PATH entry holding gh.exe into
    // child_path: that directory carries gh.exe alone, minimal leakage;
    // absent, deps publishing stays a have_gh no-op as before
    if let Some(ghdir) = std::env::var("PATH")
        .unwrap_or_default()
        .split(';')
        .map(|d| PathBuf::from(d.trim()).join("gh.exe"))
        .find(|p| p.is_file())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        child_path.push(';');
        child_path.push_str(&ghdir.to_string_lossy());
    }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = sys::env_dir(
        "FFBUILD_WORK",
        repo_root.join(format!("ffmpeg-windows-{}", arch.name())),
    );
    let dist = sys::env_dir("FFBUILD_DIST", repo_root.join("artifacts"));
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    sys::set_env(Env {
        os: Os::Windows,
        arch,
        msystem: pr.msystem.to_string(),
        pkgprefix: pr.pkgprefix.to_string(),
        pacrepo: pr.pacrepo.to_string(),
        msys2_root: root,
        prefix,
        env_prefix: format!("/{}", pr.envdir),
        prefix_win,
        bash,
        cc: cc.to_string(),
        cxx: cxx.to_string(),
        ar: "ar".to_string(),
        ranlib: "ranlib".to_string(),
        nm: "nm".to_string(),
        strip: "strip".to_string(),
        objdump: "objdump".to_string(),
        cxxrt: cxxrt.to_string(),
        gomp: RwLock::new(String::new()),
        jobs,
        work,
        dist,
        repo_root,
        child_path,
        inside_msys: std::env::var("MSYSTEM").is_ok(),
    });
    Ok(())
}

/// packages/windows.yaml 项：`name: {archs, flags}`。flags 列只作记录——
/// FFmpeg configure 旗标真源在 features/，不由包表汇集。
/// A packages/windows.yaml entry: `name: {archs, flags}`. The flags
/// field is kept for documentation only — the FFmpeg configure flag
/// source of truth lives in features/, not in this table.
fn package_rows() -> Result<Vec<crate::manifest::packages::Pkg>, String> {
    crate::manifest::packages::load("windows.yaml")
}

/// pacman -S 重试包装：--needed 幂等安装，pacover 在半安装恢复态时带
/// --overwrite '*' / pacman -S retry wrapper: idempotent --needed
/// installs; pacover carries --overwrite '*' when recovering a
/// half-installed env
fn pacman_install(e: &Env, over: &[&str], pkgs: &[&str], tries: u32) -> Result<(), String> {
    let mut args: Vec<&str> = vec!["-S", "--needed", "--noconfirm", "--noprogressbar"];
    args.extend_from_slice(over);
    args.extend_from_slice(pkgs);
    sys::retry(tries, || sys::run_ok("pacman", &args, &e.work))?;
    // 文件在位验证：pacman local DB 随 env cache 恢复，/usr 树却由
    // setup-msys2 每轮重建——DB 声称已装但文件缺失的包会被 --needed
    // 放鸽子（"up to date -- skipping"，然后 configure 报 command
    // not found）。-Qkq 只列问题项，不干净就去掉 --needed 整批强
    // 重装 / file-presence verification: the pacman local DB rides
    // the env cache while the /usr tree is rebuilt by setup-msys2
    // every run — a package whose files are missing still reads
    // "installed" to --needed ("up to date -- skipping", then
    // configure reports command not found). -Qkq lists only problem
    // entries; anything short of clean force-reinstalls the batch
    // without --needed
    let mut qk: Vec<&str> = vec!["-Qkq"];
    qk.extend_from_slice(pkgs);
    if sys::run("pacman", &qk, &e.work)? != 0 {
        let mut force: Vec<&str> = vec!["-S", "--noconfirm", "--noprogressbar"];
        force.extend_from_slice(over);
        force.extend_from_slice(pkgs);
        sys::retry(tries, || sys::run_ok("pacman", &force, &e.work))?;
    }
    Ok(())
}

/// 换 pacman 源：重写 mirrorlist.* 里每个 URL 的 scheme://host 段，
/// 保留仓库路径尾（msys/、mingw/…），套上新基址。别名：
/// ustc/tuna/aliyun→对应镜像，repo→上游源站，default→跳转器还原；
/// 其余值按原样当基址（剥 https:// 前缀和尾部 /）。对任意旧镜像
/// 幂等，只在用户点名时调用——默认不动 mirrorlist。
/// Switch the pacman mirror: rewrite the scheme://host part of every
/// URL in mirrorlist.*, keep the repo path tail (msys/, mingw/, ...) and
/// prepend the new base. Aliases: ustc/tuna/aliyun map to the well-known
/// mirrors, repo to the upstream origin, default restores the
/// redirector; anything else is taken verbatim (an https:// prefix and
/// trailing / are stripped). Idempotent against any prior mirror and
/// only applied when the user names one — mirrorlists stay untouched by
/// default.
fn apply_mirror(e: &Env, mirror: &str) {
    let base = match mirror {
        "ustc" => "mirrors.ustc.edu.cn/msys2".to_string(),
        "tuna" => "mirrors.tuna.tsinghua.edu.cn/msys2".to_string(),
        "aliyun" => "mirrors.aliyun.com/msys2".to_string(),
        "repo" | "origin" | "upstream" => "repo.msys2.org".to_string(),
        "default" | "redirector" => "mirror.msys2.org".to_string(),
        raw => raw
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string(),
    };
    if base.is_empty() {
        return;
    }
    // 逐行重写 URL 基址：https://<旧主机>/<路径尾> → https://<base>/<路径尾>
    // per-line URL-base rewrite: https://<old host>/<tail> ->
    // https://<base>/<tail>
    let rewrite = |txt: &str| -> String {
        txt.lines()
            .map(|line| {
                let Some(proto) = line.find("://") else {
                    return line.to_string();
                };
                let after = &line[proto + 3..];
                match after.find('/') {
                    Some(slash) => format!("{}{base}/{}", &line[..proto + 3], &after[slash + 1..]),
                    None => line.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + if txt.ends_with('\n') { "\n" } else { "" }
    };
    let mdir = e.msys2_root.join("etc/pacman.d");
    let mut touched = 0u32;
    if let Ok(rd) = std::fs::read_dir(&mdir) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if !name.starts_with("mirrorlist.") {
                continue;
            }
            let p = ent.path();
            if let Ok(txt) = std::fs::read_to_string(&p) {
                let out = rewrite(&txt);
                if out != txt && std::fs::write(&p, &out).is_ok() {
                    touched += 1;
                }
            }
        }
    }
    log::log(&format!("pacman mirror -> {base} ({touched} mirrorlists)"));
}

/// packages 阶段：装工具链 + packages/windows.yaml 里本架构可用的 pacman
/// 包
/// Packages stage: install the toolchain plus every pacman package from
/// packages/windows.yaml available for this arch
pub fn packages() -> Result<(), String> {
    let e = sys::env();
    fsx::install_d(&e.work)?;
    log::group_begin(&format!("MSYS2 packages ({})", e.msystem));

    // Windows git 会重写行尾，弄坏 configure 脚本。
    // Windows git rewrites line endings, which breaks configure scripts.
    for (k, v) in [("core.autocrlf", "false"), ("advice.detachedHead", "false")] {
        let _ = sys::run_ok("git", &["config", "--global", k, v], &e.work);
    }

    // 换源只在点名时做（FFBUILD_MIRROR / --mirror）；pacman 并行下载常驻
    // ——~170 个小包从几分钟变几秒钟。
    // Mirror switching happens only when named (FFBUILD_MIRROR /
    // --mirror); pacman parallel downloads stay on — ~170 small packages
    // become seconds, not minutes.
    if let Ok(m) = std::env::var("FFBUILD_MIRROR")
        && !m.trim().is_empty()
    {
        apply_mirror(e, m.trim());
    }
    let pconf = e.msys2_root.join("etc/pacman.conf");
    if let Ok(txt) = std::fs::read_to_string(&pconf)
        && !txt
            .lines()
            .any(|l| l.trim_start().starts_with("ParallelDownloads"))
    {
        let _ = std::fs::write(
            &pconf,
            txt.replacen("[options]", "[options]\nParallelDownloads = 8", 1),
        );
    }

    // 环境缓存恢复回来的子树+本地库若是半安装态（缺收尾戳）：抹掉本地库、
    // 所有 -S 走 --overwrite 覆盖重装——正确性交给 pacman。
    // If the restored env cache is half-installed (missing completion
    // stamp): wipe the local DB and let every -S run with --overwrite so
    // pacman reinstalls over whatever is on disk.
    let db = e.msys2_root.join("var/lib/pacman/local");
    let envstamp = db.join(".ff-env-complete");
    let mut pacover: Vec<&str> = Vec::new();
    if std::env::var("MSYS_ENV_RESTORED").as_deref() == Ok("true") && !envstamp.is_file() {
        log::warn("restored env cache has no completion stamp; reinstalling over it");
        fsx::rm_rf(&db);
        pacover.extend(["--overwrite", "*"]);
    }
    // 戳先摘——本轮死在安装中途时旧戳不能让下轮误判健康 / drop the stamp
    // first — a stamp restored from cache must not mark a run that dies
    // mid-install as healthy
    let _ = std::fs::remove_file(&envstamp);

    // 住在纯 MSYS 仓库里的工具（不是 mingw-w64-*）。
    // Tools living in the plain MSYS repo (not mingw-w64-*).
    pacman_install(
        e,
        &pacover,
        &[
            "make",
            "git",
            "patch",
            "diffutils",
            "p7zip",
            "unzip",
            "tar",
            "gawk",
            "perl",
            "texinfo",
            "zstd",
        ],
        3,
    )?;

    // 工具链：gdb 仅构建期诊断用（冒烟测试查栈），发货 exe 全静态见不到
    // 它。/ toolchain: gdb is build-env diagnostics only (smoke-test stack
    // traces) — the shipped exes are fully static and never see it
    let tc: Vec<String> = match e.arch {
        Arch::Arm64 => [
            "clang",
            "lld",
            "llvm",
            "libc++",
            "libunwind",
            "compiler-rt",
            "crt",
            "headers",
            "tools",
            "winpthreads",
            "windows-default-manifest",
            "llvm-openmp",
            "gdb",
            // 原生 aarch64 ccache——不走 x86 仿真，WoA 的 rdrand 雷
            // 对它不适用；装上备用，FFBUILD_CCACHE 翻 env 即开 /
            // native aarch64 ccache — no x86 emulation, so the WoA
            // rdrand landmine does not apply; installed and ready —
            // FFBUILD_CCACHE is a pure env flip away
            "ccache",
        ]
        .iter()
        .map(|p| format!("{}-{}", e.pkgprefix, p))
        .collect(),
        _ => vec![
            format!("{}-toolchain", e.pkgprefix),
            format!("{}-gdb", e.pkgprefix),
            // 用原生 mingw 版 ccache 而非 MSYS POSIX 版——后者是
            // cygwin 链接二进制，在驱动 spawn 的环境里会零输出
            // 静默退出（实证：文件在位、configure 报 C compiler
            // test failed 且 config.log 无 stderr）；原生版没有
            // cygwin DLL 依赖，整个失败类被连根除掉 /
            // the native mingw ccache instead of the MSYS POSIX one —
            // the latter is a cygwin-linked binary that exits
            // silently with zero output under the driver's spawned
            // env (empirical: the file was present yet configure
            // reported C compiler test failed with no stderr in
            // config.log); the native one has no cygwin DLL
            // dependency and the whole failure class disappears
            format!("{}-ccache", e.pkgprefix),
        ],
    };
    {
        let tcr: Vec<&str> = tc.iter().map(String::as_str).collect();
        pacman_install(e, &pacover, &tcr, 3)?;
    }

    // 仓库可用清单——包表里仓库没有的包记 skipped，不算错。
    // Repo availability list — package-table entries absent from the repo
    // are recorded as skipped, not fatal.
    let mut avail: Vec<String> = sys::capture("pacman", &["-Slq", &e.pacrepo], &e.work)?
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    avail.sort();
    fsx::write(&e.work.join("pacman-available.txt"), &avail.join("\n"))?;
    let availset: HashSet<&str> = avail.iter().map(String::as_str).collect();

    let rows = package_rows()?;
    let mut want: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for p in &rows {
        if p.archs != "all" && p.archs != e.arch.name() {
            continue;
        }
        let full = format!("{}-{}", e.pkgprefix, p.name);
        if availset.contains(full.as_str()) {
            want.push(full);
        } else {
            skipped.push(p.name.clone());
        }
    }

    log::log(&format!(
        "installing {} packages; not available for {}: {}",
        want.len(),
        e.arch.name(),
        if skipped.is_empty() {
            "none".to_string()
        } else {
            skipped.join(" ")
        }
    ));
    {
        let wr: Vec<&str> = want.iter().map(String::as_str).collect();
        if pacman_install(e, &pacover, &wr, 3).is_err() {
            log::warn("bulk installation failed; installing the packages one by one");
            for w in &want {
                if pacman_install(e, &pacover, std::slice::from_ref(&w.as_str()), 2).is_err() {
                    log::warn(&format!("package {w} could not be installed"));
                }
            }
        }
    }

    // 只统计真正装上的——没装上的包特性降级（features 探测时自然丢）。
    // Only count what really installed — a missing package degrades its
    // feature at probe time.
    let installed: HashSet<String> = sys::capture("pacman", &["-Qq"], &e.work)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    let missing: Vec<String> = rows
        .iter()
        .filter(|p| p.archs == "all" || p.archs == e.arch.name())
        .filter(|p| !installed.contains(&format!("{}-{}", e.pkgprefix, p.name)))
        .map(|p| p.name.clone())
        .collect();
    fsx::write(&e.work.join("missing-packages.txt"), &missing.join("\n"))?;
    if !missing.is_empty() {
        log::warn(&format!(
            "packages missing, their features are left out: {}",
            missing.join(" ")
        ));
    }

    // 收尾戳随本地库目录走环境缓存——下一轮命中时证明 DB+文件完整。
    // Completion stamp inside the local DB dir so it travels with the env
    // cache — its presence proves DB+files are a complete set next run.
    let _ = std::fs::File::create(&envstamp);
    let _ = sys::bash_ok(&format!("df -h {}", e.env_prefix), &e.work);
    log::group_end();
    Ok(())
}

/// FFmpeg 阶段：取源码 → features 旗标 + 平台工具链参数 → 丢包重试
/// configure → 编译安装
/// The FFmpeg stage: fetch sources → features flags + platform toolchain
/// args → drop-and-retry configure → compile+install
pub fn ffmpeg() -> Result<(), String> {
    let e = sys::env();
    // 必需的库——丢包重试不得丢它们
    // required libraries — the drop-and-retry loop must never drop them
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
    // 源码身份落盘：package 阶段（可能是独立调用）凭它命名与写
    // BUILDINFO / persist the source identity — the package stage (which
    // may be a separate invocation) names artifacts and writes BUILDINFO
    // from it
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

    // 旗标真源：features/*.yaml 解析出的 --enable/--disable 全集合
    // the flag source of truth: the full --enable/--disable set resolved
    // from features/*.yaml
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
    // 工具链直通：configure 用它们做全部探测 / toolchain passthrough:
    // configure probes everything with these
    args.extend([
        // ccache 前缀由 FFBUILD_CCACHE 控制（显式调用模式，非
        // masquerade——档案记载的 libexec 雷只在 masquerade 下炸）；
        // configure 的探针编译同样命中缓存 / the ccache prefix is gated
        // by FFBUILD_CCACHE (explicit invocation, not masquerade — the
        // documented libexec landmine only triggers in masquerade mode);
        // configure's probe compiles hit the cache too
        format!("--cc={}", sys::ccache(&e.cc)),
        format!("--cxx={}", sys::ccache(&e.cxx)),
        format!("--ar={}", e.ar),
        format!("--ranlib={}", e.ranlib),
        format!("--nm={}", e.nm),
        format!("--strip={}", e.strip),
    ]);
    if sys::which("llvm-windres").is_some() {
        args.push("--windres=llvm-windres".into());
    }
    // ARM64 runner 的 bash 本身是 x64 仿真——uname/MACHTYPE 报
    // x86_64，configure 不点 --arch 就会默认 x86_64 并因缺 nasm
    // 直接 die（实证）/ the ARM64 runner's bash is itself emulated
    // x64 — uname/MACHTYPE report x86_64, so without --arch
    // configure defaults to x86_64 and dies on missing nasm
    // (empirical)
    if e.arch == sys::Arch::Arm64 {
        args.push("--arch=aarch64".into());
        args.push("--target-os=mingw32".into());
    }
    // -lhwy 放在链接最尾巴：libjxl_cms.a 引用 __imp_hwy_*，链接器只有
    // 见到真归档才自动导入；放最后也顺带修任何 pc 顺序问题。
    // -lhwy at the very tail of the link: libjxl_cms.a references
    // __imp_hwy_* which the linker only auto-imports when the real archive
    // is seen; keeping it last also fixes any pc-ordering issue.
    let mut hwytail = " -lm".to_string();
    if crate::pkgcfg::exists("libhwy") {
        hwytail.push_str(" -lhwy");
    }
    // FFmpeg 经 check_lib 探测的库从不查 .pc，它们的私有依赖只能搭全局
    // 链接行：soxr 死在 omp_*（$GOMP，CLANGARM64 上为空），flite 的
    // cst_socket.o 死在 __imp_accept（-lws2_32）；twolame 的头文件需要
    // 每次编译都带它的静态定义。
    // Libraries FFmpeg probes via check_lib never consult a .pc, so their
    // private deps can only ride along globally: soxr dies on omp_*
    // ($GOMP, empty on CLANGARM64) and flite's cst_socket.o on __imp_accept
    // (-lws2_32); twolame's header needs its static define in every
    // compile.
    // e.gomp 是进程内 RwLock：本阶段是独立进程，fixup 里探到的运行时
    // 带不过进程边界——自己再探一次，否则 {gomp} 恒空、libsoxr 的裸
    // -lsoxr 链测死在 omp_*（实证：ucrt64 extra-libs 该位是双空格）
    // / e.gomp is an in-process RwLock: this stage is a separate
    // process and the runtime probed inside fixup cannot cross the
    // process boundary — probe again here, else {gomp} stays empty and
    // libsoxr's bare -lsoxr link test dies on omp_* (empirical: the
    // ucrt64 extra-libs slot showed a double space)
    crate::platforms::windows_deps::ensure_gomp_probe();
    let gomp = e.gomp.read().unwrap().clone();
    args.extend([
        "--extra-cflags=-DLIBTWOLAME_STATIC".into(),
        "--extra-ldflags=-static".into(),
        format!(
            "--extra-libs={}{} {} -lws2_32 -liconv -lz",
            e.cxxrt, hwytail, gomp
        ),
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

/// package 阶段：收集二进制/文档/预设 → DLL 导入白名单 → 冒烟测试 →
/// BUILDINFO → 7z + sha256
/// Package stage: collect binaries/docs/presets -> DLL import allowlist ->
/// smoke test -> BUILDINFO -> 7z + sha256
pub fn package() -> Result<(), String> {
    let e = sys::env();
    // 源码身份（ffmpeg() 落的盘；独立调用 package 时凭它命名）
    // source identity (persisted by ffmpeg(); a standalone package run
    // names its artifact from it)
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
    // 没传 --prefix：ffmpeg 装在 MSYS2 的 /usr/local（msys2 根内）
    // no --prefix was given: ffmpeg installed under MSYS2's /usr/local
    // (inside the msys2 root)
    let usrlocal = e.msys2_root.join("usr/local");
    for exe in ["ffmpeg", "ffprobe", "ffplay"] {
        let src = usrlocal.join(format!("bin/{exe}.exe"));
        if src.is_file() {
            fsx::install(&src, &pkg.join(format!("bin/{exe}.exe")))?;
        }
    }
    for f in fsx::walk(&pkg.join("bin")) {
        let _ = sys::run(
            &e.strip,
            &["--strip-unneeded", &f.to_string_lossy()],
            &e.work,
        );
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
    let presets = usrlocal.join("share/ffmpeg");
    if presets.is_dir()
        && let Ok(rd) = std::fs::read_dir(&presets)
    {
        let mut any = false;
        for ent in rd.flatten() {
            if ent.file_name().to_string_lossy().ends_with(".ffpreset") {
                fsx::install_d(&pkg.join("presets"))?;
                fsx::install(&ent.path(), &pkg.join("presets").join(ent.file_name()))?;
                any = true;
            }
        }
        let _ = any;
    }
    let license = e.work.join("ffmpeg/COPYING.GPLv3");
    if license.is_file() {
        fsx::install(&license, &pkg.join("LICENSE.txt"))?;
    }

    // 可执行文件必须能在裸 Windows 上跑：只许导入系统 DLL——绝不许导入
    // 前缀里存在的 DLL，也不许 vulkan-1/opencl（加载器 shim 要替代的驱动
    // DLL）/ the exes must run on a plain Windows machine: system DLLs
    // only — never a DLL that exists in the prefix, nor vulkan-1/opencl
    // (the driver DLLs the loader shims are meant to replace)
    log::group_begin("Imported DLLs");
    let prefixbin = e.prefix_win.join("bin");
    let mut bad: Vec<String> = Vec::new();
    for exe in fsx::walk(&pkg.join("bin")) {
        if exe.extension().map(|x| x == "exe").unwrap_or(false) {
            log::log(&format!(
                "== {}",
                exe.file_name().unwrap().to_string_lossy()
            ));
            let out = sys::capture(&e.objdump, &["-p", &exe.to_string_lossy()], &e.work)
                .unwrap_or_default();
            for l in out.lines() {
                let l = l.trim();
                if let Some(dll) = l.strip_prefix("DLL Name:") {
                    let dll = dll.trim();
                    log::log(&format!("  {dll}"));
                    if prefixbin.join(dll).exists() {
                        bad.push(format!(
                            "{}:{dll}",
                            exe.file_name().unwrap().to_string_lossy()
                        ));
                    } else {
                        match dll.to_ascii_lowercase().as_str() {
                            "vulkan-1.dll" | "opencl.dll" => bad.push(format!(
                                "{}:{dll}",
                                exe.file_name().unwrap().to_string_lossy()
                            )),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    log::group_end();
    if !bad.is_empty() {
        return Err(format!(
            "executables import DLLs that are not part of Windows (not portable): {}",
            bad.join(" ")
        ));
    }

    // pkg/bin 里的是 stripped；冒烟测试用 /usr/local/bin 的未剥离副本做
    // gdb 诊断 / pkg/bin holds the stripped copies; point the smoke test at
    // the unstripped ones in /usr/local/bin for its gdb diagnostics
    let _guard = sys::EnvGuard::set(&[(
        "FFMPEG_GDB",
        usrlocal
            .join("bin/ffmpeg.exe")
            .to_string_lossy()
            .to_string(),
    )]);
    log::group_begin("Smoke test");
    let smoke_rc = crate::stages::smoke::smoke_test(&pkg.join("bin"));
    log::group_end();
    smoke_rc?;

    // BUILDINFO 收集 + 7z 打包 + sha256 校验和
    // BUILDINFO + 7z + sha256
    let buildconf = sys::capture(
        &pkg.join("bin/ffmpeg.exe").to_string_lossy(),
        &["-hide_banner", "-buildconf"],
        &e.work,
    )
    .unwrap_or_default();
    let missing = fsx::read(&e.work.join("missing-packages.txt"));
    let dropped = fsx::read(&e.work.join("dropped-libs.txt"));
    let status = fsx::read(&e.prefix_win.join(".extras/status.txt"));
    let ccid = sys::capture(&e.cc, &["-dumpmachine"], &e.work)
        .unwrap_or_default()
        .trim()
        .to_string();
    let ccv = sys::capture(&e.cc, &["--version"], &e.work)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let info = format!(
        "FFmpeg version : {version}\nFFmpeg commit  : {commit}\nPlatform       : {platform} ({ccid}, {ccv})\nMSYS2 packages not available/installed : {}\nDropped by configure : {}\nExtras         :\n{}\n{buildconf}",
        missing.replace('\n', " ").trim(),
        dropped.replace('\n', " ").trim(),
        status
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    fsx::write(&pkg.join("BUILDINFO.txt"), &info)?;

    fsx::install_d(&e.dist)?;
    // 固实 7z（-ms=on 全档一个 solid block，压缩率最优）；msys zip 包
    // 在 arm64 runner 上缺席（spawn zip 实证），7z 双保险：MSYS2
    // 包 + GHA 镜像自带 / solid 7z (-ms=on puts the whole archive in
    // one solid block, best ratio); the msys zip package is absent on
    // the arm64 runner (spawn zip empirical) — 7z has a double source:
    // the MSYS2 package and the GHA image
    let zipname = format!("{name}.7z");
    let zippath = e.dist.join(&zipname);
    fsx::rm_rf(&zippath);
    sys::run_ok(
        &sys::sevenz()?,
        &[
            "a",
            "-t7z",
            "-mx=9",
            "-ms=on",
            &zippath.to_string_lossy(),
            &name,
        ],
        &pkgroot,
    )?;
    // sha256 伴生文件 / sha256 sidecar
    let sum = sys::sha256_file(&zippath)?;
    fsx::write(
        &e.dist.join(format!("{zipname}.sha256")),
        &format!("{sum}  {zipname}\n"),
    )?;
    let _ = sys::bash_ok(
        &format!("ls -lh '{}'", sys::win_to_posix(&e.dist, &e.msys2_root)),
        &e.work,
    );
    Ok(())
}
