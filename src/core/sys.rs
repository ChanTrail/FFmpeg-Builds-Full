//! 进程派生与运行环境
//! Process spawning and the run-time environment

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{OnceLock, RwLock};

/// 目标平台标识 / Target OS identifier
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Mac,
    Windows,
}

/// 目标架构 / Target CPU architecture
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Arm64,
}

impl Os {
    /// 表项匹配名 / name used in recipe target lists
    pub fn name(self) -> &'static str {
        match self {
            Os::Linux => "linux",
            Os::Mac => "mac",
            Os::Windows => "windows",
        }
    }
}

impl Arch {
    /// 表项匹配名 / name used in recipe target lists
    pub fn name(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Arm64 => "arm64",
        }
    }
}

impl Env {
    /// 平台清单键：features/sources/<key>.yaml 与 misc.yaml 分节名共用，
    /// 统一 <os>-<arch> 形式。macOS 拼作 macos-arm64——与 release 矩阵、
    /// README、deps/ffmpeg 产物名的既有约定一致（Os::name() 的 "mac" 只
    /// 服务配方 targets 匹配，不进平台键）。
    /// Platform manifest key shared by features/sources/<key>.yaml and the
    /// misc.yaml section names — uniformly <os>-<arch>. macOS spells
    /// macos-arm64, matching the release matrix, the README and the
    /// deps/ffmpeg asset-name conventions (Os::name()'s "mac" only serves
    /// recipe-target matching and never enters the platform key).
    pub fn plat(&self) -> String {
        let os = match self.os {
            Os::Mac => "macos",
            o => o.name(),
        };
        format!("{os}-{}", self.arch.name())
    }
}

/// 全局运行环境：一次探测、全局只读（gomp 例外，探测后写入）
/// Global run-time environment: probed once, read-only afterwards (except
/// `gomp`, filled in by the OpenMP probe)
pub struct Env {
    pub os: Os,
    pub arch: Arch,
    /// MSYSTEM 环境名（UCRT64/CLANGARM64）；非 Windows 为空
    /// MSYSTEM name (UCRT64/CLANGARM64); empty off Windows
    pub msystem: String,
    /// pacman 包名前缀与仓库名 / pacman package prefix and repo name
    pub pkgprefix: String,
    pub pacrepo: String,
    /// MSYS2 根目录（Windows 原生路径，如 D:\tools\msys64）
    /// MSYS2 root (native Windows path, e.g. D:\tools\msys64)
    pub msys2_root: PathBuf,
    /// 库安装前缀的混合形式（D:/tools/msys64/ucrt64）——写进 .pc/configure
    /// Mixed-form install prefix (D:/tools/msys64/ucrt64) — for .pc/configure
    pub prefix: String,
    /// 前缀的 POSIX 形式（/ucrt64）——bash 命令行用
    /// POSIX form of the prefix (/ucrt64) — for bash command lines
    pub env_prefix: String,
    /// 前缀的 Windows 路径形式 —— 本进程的 fs 操作用
    /// Windows form of the prefix — for this process's fs ops
    pub prefix_win: PathBuf,
    /// bash 可执行文件 / the bash executable
    pub bash: PathBuf,
    /// 工具链 / toolchain
    pub cc: String,
    pub cxx: String,
    pub ar: String,
    pub ranlib: String,
    pub nm: String,
    pub strip: String,
    pub objdump: String,
    /// C++ 运行时链接旗标 / C++ runtime link flag
    pub cxxrt: String,
    /// OpenMP 运行时（deps_fixup 探测填充）/ OpenMP runtime (deps_fixup probe)
    pub gomp: RwLock<String>,
    /// 并行任务数 / parallel jobs
    pub jobs: usize,
    /// 工作目录 / scratch dir
    pub work: PathBuf,
    /// 产物输出目录 / artifact output dir
    pub dist: PathBuf,
    /// 仓库根目录 / repository root
    pub repo_root: PathBuf,
    /// 子进程继承的额外 PATH（Windows 原生分号形式，前缀+bin 已并入）
    /// Extra PATH entries for children (Windows ';'-form, prefix bin folded in)
    pub child_path: String,
    /// 是否已在 MSYS2 shell 内（MSYSTEM 已设置）/ already inside MSYS2
    pub inside_msys: bool,
}

static G: OnceLock<Env> = OnceLock::new();

/// 初始化全局环境 / Install the global environment
pub fn set_env(e: Env) {
    let _ = G.set(e);
}

/// smoke_env：`smoke` 子命令的最小环境——只装 PATH 与临时工作目录，不做
/// 任何工具链探测。verify 的干净 runner 上没有 MSYS2/apt/brew，平台
/// init_env 必挂；冒烟本身只需要 spawn ffmpeg/ffprobe。
/// A minimal env for the `smoke` subcommand: installs PATH and a temp work
/// dir only, without any toolchain probing. Verification runners are clean
/// machines with no MSYS2/apt/brew where the platform init_env would fail;
/// the smoke test itself only needs to spawn ffmpeg/ffprobe.
pub fn smoke_env() {
    let work = std::env::temp_dir().join("ffbuild-smoke");
    // current_dir 目标必须存在——spawn 前建好 / current_dir must exist
    // before spawning
    let _ = std::fs::create_dir_all(&work);
    let (os, arch) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => (Os::Windows, Arch::X86_64),
        ("windows", "aarch64") => (Os::Windows, Arch::Arm64),
        ("linux", "x86_64") => (Os::Linux, Arch::X86_64),
        ("linux", "aarch64") => (Os::Linux, Arch::Arm64),
        ("macos", "aarch64") => (Os::Mac, Arch::Arm64),
        (o, a) => crate::log::die(&format!("unsupported host platform for smoke: {o}-{a}")),
    };
    set_env(Env {
        os,
        arch,
        msystem: String::new(),
        pkgprefix: String::new(),
        pacrepo: String::new(),
        msys2_root: PathBuf::new(),
        prefix: String::new(),
        env_prefix: String::new(),
        prefix_win: PathBuf::new(),
        // smoke 只派生 .exe/原生二进制，不经 bash；字段留空也不会被用到
        // smoke only spawns .exe/native binaries, never through bash; the
        // field stays empty and is never dereferenced
        bash: PathBuf::new(),
        cc: String::new(),
        cxx: String::new(),
        ar: String::new(),
        ranlib: String::new(),
        nm: String::new(),
        strip: String::new(),
        objdump: String::new(),
        cxxrt: String::new(),
        gomp: RwLock::new(String::new()),
        // FFBUILD_JOBS 可超订/限核——MSYS2 的 fork+I/O 停顿多，
        // -j6 超订常比核数更快；缺省探测逻辑核，兜底 4 /
        // FFBUILD_JOBS can oversubscribe or cap the core count —
        // MSYS2's fork+I/O stalls often make -j6 faster than the
        // core count; default is detected logical cores, fallback 4
        jobs: std::env::var("FFBUILD_JOBS")
            .ok()
            .and_then(|s| s.parse().ok())
            .or_else(|| std::thread::available_parallelism().map(|n| n.get()).ok())
            .unwrap_or(4),
        work,
        dist: std::env::temp_dir(),
        repo_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        child_path: std::env::var("PATH").unwrap_or_default(),
        inside_msys: false,
    });
}

/// 取全局环境 / Access the global environment
pub fn env() -> &'static Env {
    G.get().expect("env not initialised")
}

/// 读目录型环境变量：空串当作未设；相对路径落到进程 cwd 上——传给
/// `current_dir(其它目录)` 的子进程参数必须是绝对路径，否则相对串在
/// 子进程的工作目录下查无此目录（实证：CI 的 `FFBUILD_DIST: dist`
/// 让 p7zip 在 pkgroot 里找 dist/ 报 cannot open file）/ read a
/// directory env var: an empty string counts as unset, and a relative
/// value resolves against the process cwd — an argument handed to a
/// child whose current_dir sits elsewhere must be absolute, or the
/// relative string finds no such directory there (empirical: CI's
/// `FFBUILD_DIST: dist` sent p7zip hunting dist/ under pkgroot and it
/// reported 'cannot open file')
pub fn env_dir(var: &str, default: PathBuf) -> PathBuf {
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                std::env::current_dir().unwrap_or_default().join(p)
            }
        })
        .unwrap_or(default)
}

/// FFBUILD_CCACHE 非空时给编译器名加 "ccache " 前缀——显式调用模式，
/// 不是 symlink masquerade：档案里记载的 MSYS2 雷（冒名 shim 弄坏
/// gcc libexec 推导）只在 masquerade 下炸；WoA 仿真的 rdrand 崩溃是
/// arm64 专属，那边的工作流不设此变量 / prepend "ccache " to the
/// compiler name when FFBUILD_CCACHE is set non-empty — explicit
/// invocation mode, not symlink masquerade: the documented MSYS2
/// landmine (masquerade shims breaking gcc's libexec derivation) only
/// triggers in masquerade mode, and the WoA-emulation rdrand crash is
/// arm64-only — that leg's workflow simply never sets the variable
pub fn ccache(tool: &str) -> String {
    match std::env::var("FFBUILD_CCACHE") {
        Ok(v) if !v.is_empty() => format!("ccache {tool}"),
        _ => tool.to_string(),
    }
}

/// FFBUILD_SHARED 非空 = 共享构建：ffmpeg 走 --enable-shared
/// --disable-static，产物形态变成 可执行文件 + 共享库（共享库在包内
/// 平放 bin/ 旁位解析——windows PE 自动搜 exe 目录、mac @loader_path
/// rpath、linux patchelf $ORIGIN）；ELF .so 要求依赖归档是 PIC——
/// extras 的 CFLAGS 通道在 linux/mac 一律注入 -fPIC（代价≈0，静态
/// 产物通吃，recipe_sum 的 :pic 盐逼旧非 PIC 快照重建一轮）/ a
/// non-empty FFBUILD_SHARED selects a shared build: ffmpeg takes
/// --enable-shared --disable-static and the artifact becomes binaries
/// plus shared libs (libs land flat in bin/ beside the exes — Windows
/// PE searches the exe dir natively, macOS resolves @loader_path
/// rpaths, Linux gets a patchelf'd $ORIGIN); ELF .so files require PIC
/// dep archives — the extras CFLAGS channel injects -fPIC
/// unconditionally on linux/mac (~free, static products fine with it
/// too, and recipe_sum's :pic salt forces one rebuild of pre-PIC
/// snapshots)
pub fn shared() -> bool {
    std::env::var("FFBUILD_SHARED")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// FFBUILD_DEBUG 非空 = 真·调试构建：ffmpeg 走 --enable-debug=3
/// --disable-optimizations --disable-stripping，产物 -O0 全符号
/// （package 的剥符号循环同步跳过）/ a non-empty FFBUILD_DEBUG selects
/// a real debug build: ffmpeg takes --enable-debug=3
/// --disable-optimizations --disable-stripping for -O0 + full symbols
/// (the package-time strip loop is skipped in lockstep)
pub fn debug() -> bool {
    std::env::var("FFBUILD_DEBUG")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// 系统 pkg-config 的编译期默认搜索路径——读 `pkg-config --variable
/// pc_path`，跨发行版/架构都正确（自动覆盖 multiarch 目录）。进程级
/// 缓存一次探测；查不到退化为常见兜底 / the system pkg-config
/// compile-time default search path — read from `pkg-config --variable
/// pc_path`, correct across distros/arches (multiarch dirs included).
/// Probed once per process; falls back to the common paths
fn system_pc_path() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| {
        std::process::Command::new("pkg-config")
            .args(["--variable", "pc_path", "pkg-config"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/usr/lib/pkgconfig:/usr/share/pkgconfig".to_string())
    })
}

/// 拼子进程 PATH：前缀 bin + MSYS usr/bin + 原 PATH / Build the child PATH
pub fn child_path(msys2_root: &Path, env_prefix: &str) -> String {
    let prefix_bin = posix_to_win(&format!("{env_prefix}/bin"), msys2_root);
    let usr_bin = msys2_root.join("usr").join("bin");
    let old = std::env::var("PATH").unwrap_or_default();
    format!("{};{};{}", prefix_bin.display(), usr_bin.display(), old)
}

/// POSIX 路径 → Windows 路径（cygpath -w 的等价物，覆盖 <root> MSYS2 根
/// 的挂载与 cygdrive）/ POSIX → Windows path (cygpath -w equivalent for
/// the mounts under the MSYS2 <root> and cygdrive)
pub fn posix_to_win(p: &str, root: &Path) -> PathBuf {
    let p = p.replace('\\', "/");
    // /cygdrive 与裸 /d/ 两种写法 / both /cygdrive and bare /d/ spellings
    for pre in ["/cygdrive/", "/"] {
        if let Some(rest) = p.strip_prefix(pre) {
            let b: Vec<char> = rest.chars().collect();
            if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == '/' {
                return PathBuf::from(format!("{}:{}", b[0].to_ascii_uppercase(), &rest[1..]));
            }
            if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == ':' {
                return PathBuf::from(&rest[..2].to_uppercase()).join(&rest[2..]);
            }
        }
    }
    // MSYS2 根内路径 / path inside the MSYS2 root
    root.join(p.trim_start_matches('/'))
}

/// Windows 路径 → 混合形式（cygpath -m：D:/x/y）/ Windows → mixed cygpath -m
pub fn win_to_mixed(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Windows 路径 → POSIX 形式（cygpath -u；<root> MSYS2 根内先折成 /x，
/// 其余按盘符）/ Windows → POSIX (cygpath -u: paths inside the MSYS2
/// <root> collapse to /x, others map to /drive/x)
pub fn win_to_posix(p: &Path, root: &Path) -> String {
    let p = win_to_mixed(p);
    let r = win_to_mixed(root);
    if let Some(rest) = p.strip_prefix(&r) {
        return format!("/{}", rest.trim_start_matches('/'));
    }
    let c: Vec<char> = p.chars().collect();
    if c.len() >= 2 && c[0].is_ascii_alphabetic() && c[1] == ':' {
        return format!("/{}{}", c[0].to_ascii_lowercase(), &p[2..]);
    }
    p
}

/// 程序的可执行形式：(程序, 完整 argv)。本进程是原生二进制，直接
/// exec 一个无扩展名脚本（configure/meson/autoreconf/…）会 ENOENT；
/// 这类程序经 `bash -c 'exec "$@"'` 派生——argv 逐字透传不涉引号。
/// Executable form of prog: (program, full argv). This process is a native
/// binary, so exec'ing an extension-less script (configure/meson/
/// autoreconf/...) would ENOENT; those go through `bash -c 'exec "$@"'`,
/// which passes argv verbatim with no quoting involved.
fn resolve(prog: &str, args: &[&str]) -> (String, Vec<String>) {
    /// 路径解析为可直接派生的原生可执行？/ does the path resolve to a
    /// directly spawnable native executable?
    fn is_native(p: &std::path::Path) -> bool {
        p.extension()
            .map(|x| {
                let x = x.to_string_lossy().to_lowercase();
                x == "exe" || x == "bat" || x == "cmd" || x == "com"
            })
            .unwrap_or(false)
    }
    let direct = if prog.contains('/') || prog.contains('\\') {
        let p = Path::new(prog);
        p.is_file() && is_native(p)
    } else {
        match which(prog) {
            // PATH 上找到无扩展名脚本 → bash；找不到就交给 OS 报错
            // a bare-name script on PATH → bash; not found → let the OS
            // report the error itself
            Some(p) => is_native(&p),
            None => true,
        }
    };
    if direct {
        (
            prog.to_string(),
            args.iter().map(|s| s.to_string()).collect(),
        )
    } else {
        let e = env();
        // 经 bash 时路径参数必须是 POSIX 形式（D:\x 在 bash 里不解析）
        // a path argument must be POSIX when routed through bash (D:\x does
        // not resolve there)
        let pprog = if prog.contains('/') || prog.contains('\\') {
            win_to_posix(Path::new(prog), &e.msys2_root)
        } else {
            prog.to_string()
        };
        let mut v: Vec<String> = vec!["-c".into(), "exec \"$@\"".into(), "ffbuild".into(), pprog];
        v.extend(args.iter().map(|s| s.to_string()));
        (e.bash.to_string_lossy().to_string(), v)
    }
}

/// 拼一条标准子进程命令（继承全局环境变量与 child_path；脚本自动经 bash）
/// Build a Command carrying the global env vars and the child PATH
/// (scripts are routed through bash automatically)
pub fn cmd(prog: &str, args: &[&str]) -> Command {
    let e = env();
    let (p, a) = resolve(prog, args);
    let mut c = Command::new(p);
    c.args(a)
        .env("PATH", &e.child_path)
        .env("CC", &e.cc)
        .env("CXX", &e.cxx)
        .env("AR", &e.ar)
        .env("RANLIB", &e.ranlib)
        .env("NM", &e.nm)
        .env("STRIP", &e.strip)
        .env("OBJDUMP", &e.objdump)
        .env("CXXRT", &e.cxxrt)
        .env("PREFIX", &e.prefix)
        .env("JOBS", e.jobs.to_string())
        .env("ARCH", e.arch.name())
        .env("OS", e.os.name())
        .env("PKG_CONFIG", "pkg-config")
        .env("MSYSTEM", &e.msystem)
        .env("MSYSTEM_PREFIX", &e.env_prefix)
        .env("MSYS2_ROOT", win_to_mixed(&e.msys2_root))
        .env("ENV_PREFIX", &e.env_prefix)
        // git>=2.31 的全局 advice 开关：detached HEAD 说教、
        // init.defaultBranch 提示等 hint: 块在所有子孙进程一次掐掉
        // git>=2.31's global advice switch: the detached-HEAD lecture,
        // init.defaultBranch notices and every other hint: block are
        // silenced for all descendants in one shot
        .env("GIT_ADVICE", "0");
    if !e.msystem.is_empty() {
        c.env("CHERE_INVOKING", "1");
    }
    if e.os != Os::Windows {
        // POSIX：前缀不进 pkg-config 默认搜索路径，显式给 LIBDIR；
        // aclocal 要看到前缀装的宏；CMake 4.x 移除了 <3.5 最低版本兼容
        // POSIX: the prefix is not on pkg-config's default search path so
        // LIBDIR is passed explicitly; aclocal must see prefix-installed
        // macros; CMake 4.x dropped <3.5 minimum-version compat
        let libdir = if e.os == Os::Linux {
            // Linux 的系统 .pc 是刻意 apt 装的开发包（x11/glib/vulkan 链）
            // ——LIBDIR 整体替换会把它们全部遮没（vdpau 的 x11、lensfun 的
            // glib-2.0 全查无）；前缀排第一，自建仓仍旧优先 / the system
            // .pc files on Linux are deliberately apt-installed dev
            // packages (the x11/glib/vulkan set) — a wholesale LIBDIR
            // replace hides every one of them (vdpau's x11, lensfun's
            // glib-2.0 all unresolvable); the prefix stays first so our
            // own builds still win
            format!(
                "{}/lib/pkgconfig:{}/share/pkgconfig:{}",
                e.prefix,
                e.prefix,
                system_pc_path()
            )
        } else {
            // macOS 保持整体替换：/opt/homebrew 的 .pc 一概不见，防 brew
            // 渗透 / macOS keeps the wholesale replace: no /opt/homebrew
            // .pc can leak in
            format!("{}/lib/pkgconfig:{}/share/pkgconfig", e.prefix, e.prefix)
        };
        c.env("PKG_CONFIG_LIBDIR", libdir)
            .env(
                "ACLOCAL_PATH",
                format!("{}/share/aclocal:/usr/share/aclocal", e.prefix),
            )
            .env("CMAKE_POLICY_VERSION_MINIMUM", "3.5");
    }
    if e.os == Os::Linux {
        // rustup/cargo-c 的家目录钉在 /opt 下随 deps 快照往返；macOS 不钉
        // ——/opt 在 runner 上归 root，工具链走镜像自带的 ~/.rustup
        // the rustup/cargo-c homes are pinned under /opt so they ride the
        // deps snapshot; macOS does not pin — /opt is root-owned there and
        // the toolchain lives in the image-provided ~/.rustup
        c.env("CARGO_HOME", "/opt/cargo")
            .env("RUSTUP_HOME", "/opt/rustup");
    }
    c
}

/// CI 输出闸门的小体量阈值：捕获输出小于它仍原样放出（版本/提示行不
/// 丢），超出的成功即吞、失败回放尾部 / CI output-gate size threshold:
/// captured output below it still passes verbatim (version/hint lines
/// are kept); larger output is dropped on success and tail-replayed on
/// failure
const CI_GATE_SMALL: usize = 4 * 1024;

/// 闸内命令失败时回放给控制台的输出行数 / how many tail lines a gated
/// command's failure replays to the console
const CI_GATE_TAIL: usize = 40;

/// 本进程是否该闸住子进程输出：CI 下 runner 日志管道的背压在单步数万
/// 行后堵死写端（见 run_tee 注释）。FFBUILD_CHILD 标记的
/// recipe-run/postfix-run 子进程（run_tee spawn 时注入）不闸——其输出
/// 经管道喂给父进程的 <name>.log，抑制由父进程那一头做
/// whether this process should gate child output: under CI the runner's
/// log channel backpressures once a step accumulates tens of thousands
/// of lines (see run_tee). recipe-run/postfix-run children marked with
/// FFBUILD_CHILD (injected by run_tee at spawn) are never gated — their
/// output rides the pipe into the parent's <name>.log and suppression
/// happens on the parent's side
fn ci_gate() -> bool {
    std::env::var_os("GITHUB_ACTIONS").is_some() && std::env::var_os("FFBUILD_CHILD").is_none()
}

/// 文本末尾 n 个非空行 / the last n non-empty lines of a text
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// 闸制模式跑完并按体量处置捕获输出：小体量原样放出；大体量成功只留
/// 一行体量摘要，失败回放尾部 CI_GATE_TAIL 行现场。等待期间每 30s
/// 打一次报活，长命令（apt-get/pacman/brew）不至于看起来像冻住
/// run to completion in gated mode and dispose of the captured output
/// by size: small payloads pass verbatim; large ones leave a one-line
/// size digest on success and replay a CI_GATE_TAIL-line tail on
/// failure. A 30-second liveness line keeps long commands
/// (apt-get/pacman/brew) from looking frozen
fn run_gated(c: &mut Command, desc: &str) -> Result<i32, String> {
    // 报活线程：等待期间每 30s 一行 / liveness thread: one line every
    // 30 s while the command runs
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let label = desc.to_string();
    let watcher = std::thread::spawn(move || {
        let mut secs = 0u32;
        while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
            rx.recv_timeout(std::time::Duration::from_secs(30))
        {
            secs += 30;
            crate::log::log(&format!("… {label}: still running ({secs}s)"));
        }
    });
    let out = c.output().map_err(|e| format!("spawn {desc}: {e}"))?;
    let _ = tx.send(());
    let _ = watcher.join();
    let code = out.status.code().unwrap_or(-1);
    let total = out.stdout.len() + out.stderr.len();
    if code == 0 && total < CI_GATE_SMALL {
        // 小体量：原样放出（stdout 在前、stderr 在后）/ small payload:
        // verbatim (stdout first, stderr second)
        let _ = std::io::stdout().write_all(&out.stdout);
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().write_all(&out.stderr);
        let _ = std::io::stderr().flush();
    } else if code != 0 {
        // 失败：回放合并输出的尾部现场 / failure: replay the tail of the
        // merged streams
        let mut merged = String::from_utf8_lossy(&out.stdout).into_owned();
        merged.push_str(&String::from_utf8_lossy(&out.stderr));
        println!("{}", tail_lines(&merged, CI_GATE_TAIL));
    } else {
        crate::log::log(&format!(
            "… {desc}: {} output suppressed",
            human_size(total as u64)
        ));
    }
    Ok(code)
}

/// 直接派生程序并等待完成；输出直通本进程 stdio。CI 下（且非
/// FFBUILD_CHILD 子进程）改为捕获闸门：小体量放出、大体量吞掉只留
/// 摘要+报活、失败回放尾部——见 ci_gate/run_gated
/// Spawn a program and wait; output passes through to our stdio. Under
/// CI (and not inside an FFBUILD_CHILD subprocess) it goes through the
/// capture gate instead: small payloads pass, large ones are swallowed
/// to a digest plus liveness lines, failures replay the tail — see
/// ci_gate/run_gated
pub fn run(prog: &str, args: &[&str], dir: &Path) -> Result<i32, String> {
    let mut c = cmd(prog, args);
    c.current_dir(dir);
    if ci_gate() {
        // 心跳标签：bash -c 取脚本首部（基名全是 bash 没信息量），其余
        // 取程序基名 / heartbeat label: the script head for bash -c (the
        // basename is always bash and says nothing), the program
        // basename otherwise
        let desc = if args.first() == Some(&"-c") && args.len() >= 2 {
            format!("bash {}", args[1].chars().take(40).collect::<String>())
        } else {
            Path::new(prog)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| prog.to_string())
        };
        return run_gated(&mut c, &desc);
    }
    c.status()
        .map(|s| s.code().unwrap_or(-1))
        .map_err(|e| format!("spawn {prog}: {e}"))
}

/// 非零退出视为失败 / Fail on a non-zero exit status
pub fn run_ok(prog: &str, args: &[&str], dir: &Path) -> Result<(), String> {
    match run(prog, args, dir)? {
        0 => Ok(()),
        rc => Err(format!("{prog} {args:?} exited {rc}")),
    }
}

/// 经 bash -c 执行脚本串（MSYS2/系统 bash），继承全局环境
/// Run a script line through bash -c (MSYS2/system bash), inheriting the env
pub fn bash(script: &str, dir: &Path) -> Result<i32, String> {
    let e = env();
    run(e.bash.to_str().unwrap(), &["-c", script], dir)
}

/// 非零退出视为失败的 bash 执行 / bash run that fails on non-zero
pub fn bash_ok(script: &str, dir: &Path) -> Result<(), String> {
    match bash(script, dir)? {
        0 => Ok(()),
        rc => Err(format!("bash '{script}' exited {rc}")),
    }
}

/// 捕获 stdout（stderr 丢弃）/ Capture stdout (stderr discarded)
pub fn capture(prog: &str, args: &[&str], dir: &Path) -> Result<String, String> {
    let mut c = cmd(prog, args);
    c.current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let out = c.output().map_err(|e| format!("spawn {prog}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{prog} {args:?} exited {:?}", out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).replace('\r', ""))
}

/// 宽松捕获：非零退出也拿回已产出的 stdout——mac nm 撞到不可解析成员
/// 会 exit 1 但 stdout 照常吐符号表，整批丢掉正好哑掉 dedup（实证）。仅
/// spawn 失败才 Err；调用方按空输出自行判死 / lax capture: a non-zero
/// exit still hands back whatever stdout was produced — mac nm exits 1
/// on an unparseable member yet keeps printing the symbol table, and
/// discarding it wholesale is exactly what muted the dedup (empirical).
/// Only a spawn failure errors; callers judge death by empty output
pub fn capture_lax(prog: &str, args: &[&str], dir: &Path) -> Result<(String, i32), String> {
    let mut c = cmd(prog, args);
    c.current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let out = c.output().map_err(|e| format!("spawn {prog}: {e}"))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).replace('\r', ""),
        out.status.code().unwrap_or(-1),
    ))
}

/// 捕获 bash 输出 / Capture bash output
pub fn capture_bash(script: &str, dir: &Path) -> Result<String, String> {
    let e = env();
    capture(e.bash.to_str().unwrap(), &["-c", script], dir)
}

/// 捕获原始字节 stdout（二进制安全；stderr 丢弃）
/// Capture raw stdout bytes (binary-safe; stderr discarded)
pub fn capture_bytes(prog: &str, args: &[&str], dir: &Path) -> Result<Vec<u8>, String> {
    let mut c = cmd(prog, args);
    c.current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let out = c.output().map_err(|e| format!("spawn {prog}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{prog} {args:?} exited {:?}", out.status.code()));
    }
    Ok(out.stdout)
}

/// CI 下 run_tee 不回显时的进度心跳间隔：每往日志文件写满这么多字节
/// 打一行 / heartbeat interval while run_tee's CI echo is off: one line
/// per this many bytes appended to the log file
const TEE_HEARTBEAT: usize = 256 * 1024;

/// 回显抑制期的进度心跳：报 <tag> 日志文件已收字节数
/// progress heartbeat while the echo is suppressed: report how many
/// bytes the <tag> log file has taken
fn tee_beat(tag: &str, written: usize) {
    crate::log::log(&format!("… {tag}: {} written", human_size(written as u64)));
}

/// 运行并把 stdout 同时写入文件（tee 语义；stderr 也并入同一日志——
/// 诊断完整性优先于两流的严格时序）。
/// CI 下完全不回显子进程输出：Actions 的步骤日志走带背压的上传管道，
/// runner 缓冲攒满（实测单步累计数万行后）写端就被拖住——回显一堵，
/// 读循环停转、子进程管道写满，构建连锁锁死（"日志不动、构建也不动"）。
/// 改为只写日志文件并按 TEE_HEARTBEAT 打进度心跳——控制台退化为纯
/// 进度条；完整输出仍在 <name>.log/日志包，失败明细由 error_tail 兜底。
/// 非 CI 照常逐字节回显。
/// Run with stdout teed to a file (stderr joins the same log —
/// diagnostic completeness beats strict interleave order).
/// Under CI nothing of the child output is echoed: the Actions step log
/// rides a backpressured upload channel whose runner buffer fills once
/// a step accumulates tens of thousands of lines — a stalled echo stops
/// the read loop, fills the child's pipe, and deadlocks the build with
/// it ("log frozen, build frozen"). Instead only the log file grows
/// while TEE_HEARTBEAT progress lines keep the console a pure progress
/// display; full output still lands in <name>.log / the log bundle and
/// error_tail prints the failure tail. Outside CI every byte echoes.
pub fn run_tee(prog: &str, args: &[&str], dir: &Path, logfile: &Path) -> Result<i32, String> {
    let ci = std::env::var_os("GITHUB_ACTIONS").is_some();
    let tag = logfile
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tee.log".to_string());
    let mut c = cmd(prog, args);
    c.current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // FFBUILD_CHILD 标记子进程身份：其内部 sys::run 必须保持继承
        // stdio，输出经本管道喂回给本函数写日志；闸门只在父进程生效
        // mark the child context: its inner sys::run must keep
        // inheriting stdio so output rides this pipe back for the log
        // file; the CI gate applies to the parent side only
        .env("FFBUILD_CHILD", "1");
    let mut ch = c.spawn().map_err(|e| format!("spawn {prog}: {e}"))?;
    let mut so = ch.stdout.take().unwrap();
    let mut se = ch.stderr.take().unwrap();
    let mut f = std::fs::File::create(logfile).map_err(|e| format!("{logfile:?}: {e}"))?;
    // CI 回显抑制期的时间心跳：TEE_HEARTBEAT 字节里程碑只在子进程
    // 有输出时触发——安静期（链接、长编译单元）控制台彻底静默，像
    // 冻住。补一条 30s 报活线，字节数由读写循环经共享原子计数喂入 /
    // a time-based heartbeat for the CI echo-suppressed stretch:
    // TEE_HEARTBEAT byte milestones only fire while the child talks —
    // quiet stretches (link steps, long compile units) leave the
    // console dead silent and looking hung. A 30s liveness line fixes
    // it; the byte counts are fed by the reader loops via shared
    // atomics
    let written_out = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let written_err = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (beat_tx, beat_rx) = std::sync::mpsc::channel::<()>();
    let beat = if ci {
        let tag_b = tag.clone();
        let (wo, we) = (written_out.clone(), written_err.clone());
        Some(std::thread::spawn(move || {
            let mut secs = 0u32;
            while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                beat_rx.recv_timeout(std::time::Duration::from_secs(30))
            {
                secs += 30;
                let total = wo.load(std::sync::atomic::Ordering::Relaxed)
                    + we.load(std::sync::atomic::Ordering::Relaxed);
                crate::log::log(&format!(
                    "… {tag_b}: {} written, still running ({secs}s)",
                    human_size(total as u64)
                ));
            }
        }))
    } else {
        None
    };
    // stderr 读线程：必须与 stdout 并行排空——管道写满而没人读时两条流
    // 会互相堵死；同写日志文件并回显本进程 stderr
    // the stderr reader thread must drain in parallel with stdout — a full
    // pipe nobody reads deadlocks both streams; it appends to the same
    // log file and echoes to our own stderr
    let mut f_err = f.try_clone().map_err(|e| format!("{logfile:?}: {e}"))?;
    let tag_err = tag.clone();
    let t = std::thread::spawn(move || {
        let mut buf = [0u8; 65536];
        let mut wr = std::io::stderr();
        let (mut written, mut hidden) = (0usize, 0usize);
        loop {
            match se.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = f_err.write_all(&buf[..n]);
                    written += n;
                    written_err.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                    if !ci {
                        let _ = wr.write_all(&buf[..n]);
                        let _ = wr.flush();
                    } else {
                        hidden += n;
                        if hidden >= TEE_HEARTBEAT {
                            tee_beat(&tag_err, written);
                            hidden = 0;
                        }
                    }
                }
            }
        }
        // 收尾补一行总量：让最后的静默段在日志里也看得出体量
        // final total on stream end so the last quiet stretch still
        // shows its volume in the log
        if hidden > 0 {
            tee_beat(&tag_err, written);
        }
    });
    let mut buf = [0u8; 65536];
    let mut wr = std::io::stdout();
    let (mut written, mut hidden) = (0usize, 0usize);
    loop {
        match so.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let _ = f.write_all(&buf[..n]);
                written += n;
                written_out.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                if !ci {
                    let _ = wr.write_all(&buf[..n]);
                    let _ = wr.flush();
                } else {
                    hidden += n;
                    if hidden >= TEE_HEARTBEAT {
                        tee_beat(&tag, written);
                        hidden = 0;
                    }
                }
            }
            Err(_) => break,
        }
    }
    // 同上的收尾总量 / same final-total line as the stderr thread
    if hidden > 0 {
        tee_beat(&tag, written);
    }
    let _ = t.join();
    // 子进程输出已尽：停表收编心跳线程（send 即唤醒，不等下一个
    // 30s 刻度）/ child output is done: stop and collect the
    // heartbeat thread (the send wakes it now, not on the next 30s
    // tick)
    let _ = beat_tx.send(());
    if let Some(h) = beat {
        let _ = h.join();
    }
    ch.wait()
        .map(|s| s.code().unwrap_or(-1))
        .map_err(|e| format!("wait {prog}: {e}"))
}

/// 临时环境变量覆盖：RAII——drop 时恢复原值。shell 的
/// `VAR=x cmd ...` 前缀语义的进程内等价物。
/// Scoped env-var overrides: RAII — restores the previous values on drop.
/// The in-process equivalent of shell's `VAR=x cmd ...` prefix.
pub struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    /// 设置并接管若干环境变量 / set and own some env vars
    pub fn set(vars: &[(&str, String)]) -> Self {
        let mut saved = Vec::new();
        for (k, v) in vars {
            saved.push((k.to_string(), std::env::var(k).ok()));
            // SAFETY：本程序单线程驱动、子进程同步派生，改环境无并发读写
            // SAFETY: the driver is single-threaded and children are spawned
            // synchronously — no concurrent env access
            unsafe { std::env::set_var(k, v) };
        }
        EnvGuard { saved }
    }

    /// 在现有值上追加（分隔符为空格）/ append to the existing value
    /// (space-separated)
    pub fn append(vars: &[(&str, &str)]) -> Self {
        let mut saved = Vec::new();
        for (k, v) in vars {
            let old = std::env::var(k).unwrap_or_default();
            saved.push((k.to_string(), std::env::var(k).ok()));
            // SAFETY：同上 / SAFETY: as above
            unsafe { std::env::set_var(k, format!("{old} {v}").trim()) };
        }
        EnvGuard { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.saved.drain(..) {
            // SAFETY：同上 / SAFETY: as above
            match v {
                Some(x) => unsafe { std::env::set_var(&k, x) },
                None => unsafe { std::env::remove_var(&k) },
            }
        }
    }
}

/// 失败重试 n 次（间隔 10 秒）/ Retry a closure up to n times (10 s apart)
pub fn retry<F: FnMut() -> Result<(), String>>(n: u32, mut f: F) -> Result<(), String> {
    for i in 1..=n {
        match f() {
            Ok(()) => return Ok(()),
            Err(e) => {
                if i >= n {
                    return Err(e);
                }
                crate::log::log(&format!("[retry] attempt {}/{n}: {e}", i + 1));
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
        }
    }
    unreachable!()
}

/// 在子进程 PATH 里找可执行文件 / Look an executable up in the child PATH
pub fn which(prog: &str) -> Option<PathBuf> {
    // child_path 分隔符按平台：Windows ';'，POSIX ':'（不能在 Windows 上
    // 用 ':'——驱动器字母会被拆碎）/ separator by platform: ';' on Windows,
    // ':' on POSIX (a ':' split would shred Windows drive letters)
    let sep = if cfg!(windows) { ';' } else { ':' };
    let pathext: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .map(|s| s.to_string())
        .collect();
    for d in env().child_path.split(sep) {
        let d = d.trim();
        if d.is_empty() {
            continue;
        }
        for ext in ["", ".exe"] {
            let p = Path::new(d).join(format!("{prog}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
        // 无扩展名时按 PATHEXT 试探 / try PATHEXT when no hit yet
        if !prog.contains('.') {
            for ext in &pathext {
                let p = Path::new(d).join(format!("{prog}{}", ext.to_lowercase()));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// 7-Zip 解析：子进程 PATH 的 7z/7za/7zr，最后退 GHA Windows 镜像预装
/// 位置（`C:\Program Files\7-Zip`，不经 MSYS2 PATH 也能命中）/
/// resolve 7-Zip: 7z/7za/7zr on the child PATH, then the preinstalled
/// location in the GHA Windows image (`C:\Program Files\7-Zip` —
/// reachable even when MSYS2's PATH shadows it)
pub fn sevenz() -> Result<String, String> {
    for c in ["7z", "7za", "7zr"] {
        if let Some(p) = which(c) {
            return Ok(p.to_string_lossy().to_string());
        }
    }
    for p in [
        "C:/Program Files/7-Zip/7z.exe",
        "C:/Program Files (x86)/7-Zip/7z.exe",
    ] {
        if Path::new(p).is_file() {
            return Ok(p.to_string());
        }
    }
    Err("7z not found (child PATH 7z/7za/7zr; C:/Program Files/7-Zip)".into())
}

/// sha256：优先 sha256sum（GNU），退回 shasum -a 256（macOS），再退 openssl
/// sha256 of a byte slice: sha256sum → shasum -a 256 → openssl
pub fn sha256_data(data: &[u8]) -> Result<String, String> {
    for (prog, args) in [
        ("sha256sum", vec![] as Vec<&str>),
        ("shasum", vec!["-a", "256"]),
        ("openssl", vec!["dgst", "-sha256", "-r"]),
    ] {
        if which(prog).is_none() {
            continue;
        }
        let mut c = cmd(prog, &args);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Ok(mut ch) = c.spawn() {
            ch.stdin.take().unwrap().write_all(data).ok();
            if let Ok(out) = ch.wait_with_output()
                && out.status.success()
            {
                let s = String::from_utf8_lossy(&out.stdout);
                return Ok(s.split_whitespace().next().unwrap_or("").to_string());
            }
        }
    }
    Err("no sha256 tool available".into())
}

/// 文件内容的 sha256 / sha256 of a file's contents
pub fn sha256_file(p: &Path) -> Result<String, String> {
    sha256_data(&std::fs::read(p).map_err(|e| format!("{p:?}: {e}"))?)
}

/// 字节数 → 人类可读（numfmt --to=iec-i 的等价物）
/// Human-readable byte count (numfmt --to=iec-i equivalent)
pub fn human_size(b: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1}{}", units[i])
}

/// 日志文件末尾 <max_lines> 个非空行逐行原样拼出——CI 下 run_tee 不回显
/// 子进程输出，命令失败时拿它把现场打到控制台（区别于 extras 的
/// error_tail：那个是 400 字符单行摘要，这个是全量尾部）
/// last <max_lines> non-empty lines of <logfile>, joined verbatim —
/// run_tee does not echo child output under CI, so a failed command
/// prints its scene through this (unlike extras' error_tail, which is
/// a 400-char one-line digest; this is the verbatim tail)
pub fn log_tail(logfile: &Path, max_lines: usize) -> String {
    tail_lines(&crate::fsx::read(logfile), max_lines)
}
