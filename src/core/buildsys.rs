//! 构建系统封装：autotools/cmake/meson 的标准静态构建
//! Build-system wrappers: standard static builds for autotools/cmake/meson

use crate::fsx;
use crate::sys;
use std::path::Path;

/// iconv_tail：前缀里存在自编 libiconv.a 时，配方的"自身产出"链接需要
/// -liconv 收尾——我们的 iconv.h 把 iconv_* 改名成 libiconv_*，经 -I
/// 前缀吃到它的 TU 全在调改名后符号；dep .a 的未决引用由 ffmpeg 终链
/// -liconv 兜底，但配方内部 exe 链接（pacat/xmllint/glib 工具这类）不带
/// 它即 undefined / when our own libiconv.a sits in the prefix, a
/// recipe's self-produced links need a trailing -liconv — our iconv.h
/// renames iconv_* to libiconv_*, so every TU consuming it via the -I
/// prefix calls the renamed symbols; dep .a's dangling refs are settled
/// by ffmpeg's final-link -liconv, but a recipe's internal exe links
/// (pacat/xmllint/glib tools and the like) die undefined without it
fn iconv_tail() -> bool {
    let e = sys::env();
    !e.prefix.is_empty() && Path::new(&e.prefix).join("lib/libiconv.a").is_file()
}

/// cmake_build [args]：cmake 标准配方构建（静态、PIC、关 IPO/LTO）
/// cmake_build [args]: standard cmake recipe build (static, PIC, IPO/LTO off)
pub fn cmake_build(src: &Path, args: &[&str]) -> Result<(), String> {
    let e = sys::env();
    let mut v: Vec<String> = vec![
        "-S".into(),
        ".".into(),
        "-B".into(),
        "build".into(),
        "-DCMAKE_BUILD_TYPE=Release".into(),
        format!("-DCMAKE_INSTALL_PREFIX={}", e.prefix),
        "-DCMAKE_INSTALL_LIBDIR=lib".into(),
        format!("-DCMAKE_PREFIX_PATH={}", e.prefix),
        "-DBUILD_SHARED_LIBS=OFF".into(),
        "-DCMAKE_POSITION_INDEPENDENT_CODE=ON".into(),
        "-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=OFF".into(),
        "-DCMAKE_POLICY_VERSION_MINIMUM=3.5".into(),
    ];
    // -liconv 进 CMAKE_*_STANDARD_LIBRARIES：cmake 的"标准库"槽恒落
    // 链接行尾（目标自身库之后）——改名符号的未决引用在此满足；
    // TRY_COMPILE 探针同样带它，无害 / -liconv via
    // CMAKE_*_STANDARD_LIBRARIES: cmake's "standard libs" slot always
    // lands at the link-line tail (after the target's own libs) —
    // pending renamed-symbol refs resolve there; TRY_COMPILE probes
    // carry it too, harmlessly
    if iconv_tail() {
        v.push("-DCMAKE_C_STANDARD_LIBRARIES=-liconv".into());
        v.push("-DCMAKE_CXX_STANDARD_LIBRARIES=-liconv".into());
    }
    if sys::which("ninja").is_some() {
        v.insert(4, "-G".into());
        v.insert(5, "Ninja".into());
    }
    v.extend(args.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = v.iter().map(String::as_str).collect();
    sys::run_ok("cmake", &refs, src)?;
    sys::run_ok(
        "cmake",
        &["--build", "build", "-j", &e.jobs.to_string()],
        src,
    )?;
    // 生成器自己的 install 目标任何 cmake 版本都认（cmake --install 要 3.15+）
    // the generator's own install target works on any cmake vintage
    sys::run_ok("cmake", &["--build", "build", "--target", "install"], src)
}

/// refresh_config_scripts：内置 config.guess/config.sub 不认识当今三元组时
/// 用系统 autotools 文件刷新 / refresh bundled config.guess/config.sub from
/// the system's autotools files when the bundled guesser is too old
pub fn refresh_config_scripts(src: &Path) -> Result<(), String> {
    let cands: Vec<std::path::PathBuf> = fsx::walk(src)
        .into_iter()
        .filter(|p| {
            p.file_name()
                .map(|n| n == "config.guess" || n == "config.sub")
                .unwrap_or(false)
        })
        .collect();
    // 自带的能猜出构建机就不用刷 / skip when the bundled guesser still works
    let mut failed = false;
    for cf in &cands {
        if cf.file_name().map(|n| n == "config.guess").unwrap_or(false)
            && sys::run("sh", &[&cf.to_string_lossy()], src).unwrap_or(1) != 0
        {
            failed = true;
        }
    }
    if !failed {
        return Ok(());
    }
    let e = sys::env();
    let dirs = [
        "/usr/share/libtool/build-aux/",
        "/usr/share/gettext/config/",
        "/opt/homebrew/share/libtool/build-aux/",
        "/usr/local/share/libtool/build-aux/",
        "/usr/share/misc/",
    ];
    for cf in cands {
        let base = cf.file_name().unwrap().to_string_lossy().to_string();
        // automake 目录按版本通配 / automake dirs matched by version glob
        let mut pool: Vec<std::path::PathBuf> = dirs
            .iter()
            .map(|d| sys::posix_to_win(d.trim_end_matches('/'), &e.msys2_root).join(&base))
            .collect();
        for pat in fsx::walk(&sys::posix_to_win("/usr/share", &e.msys2_root)) {
            if pat.file_name().map(|n| n == base.as_str()).unwrap_or(false) {
                pool.push(pat);
            }
        }
        for cand in pool {
            if cand.is_file() && std::fs::copy(&cand, &cf).is_ok() {
                break;
            }
        }
    }
    Ok(())
}

/// autotools_build [args]：autotools 标准配方构建（无 configure 时
/// autogen/autoreconf）/ standard autotools build (autogen/autoreconf when
/// no configure)
pub fn autotools_build(src: &Path, args: &[&str]) -> Result<(), String> {
    autotools_build_vars(src, args, &[])
}

/// autotools_build_vars [args, mvars]：同 autotools_build，另把 mvars
/// 作为命令行变量传给 make 与 make install（命令行变量压过 Makefile
/// 赋值——例如 xvid 的 `$(LN_S)` 探测出来是不带 -f 的 `ln -s`，撞
/// 已存在文件即 Error，传 LN_S="ln -sf" 可解）
/// Same as autotools_build, plus mvars forwarded as command-line
/// variables to both `make` and `make install` (command-line vars beat
/// Makefile assignments — e.g. xvid's `$(LN_S)` resolves to a
/// flag-less `ln -s` that dies on existing files; LN_S="ln -sf" fixes
/// it)
pub fn autotools_build_vars(src: &Path, args: &[&str], mvars: &[&str]) -> Result<(), String> {
    let e = sys::env();
    // -liconv 经 env LIBS 进所有 automake 链接行尾（LIBS 恒为链接表
    // 末位，configure 的 @LIBS@ 替换与 make 规则同源）——内部 exe 的
    // 改名符号引用在此满足；前缀无 libiconv.a 时不设，省得 -liconv
    // 悬空 / -liconv via env LIBS lands at every automake link-line
    // tail (LIBS is always the last entry; configure's @LIBS@
    // substitution and make rules share the source) — renamed-symbol
    // refs in internal exes resolve there; skipped when no libiconv.a
    // sits in the prefix, so -liconv never dangles
    if iconv_tail() {
        let cur = std::env::var("LIBS").unwrap_or_default();
        if !cur.contains("-liconv") {
            // SAFETY：配方串行执行、无并发任务 / SAFETY: recipes run
            // serially, no concurrent tasks
            unsafe { std::env::set_var("LIBS", format!("{cur} -liconv").trim()) };
        }
    }
    refresh_config_scripts(src)?;
    if !src.join("configure").is_file() {
        if src.join("autogen.sh").is_file() {
            sys::bash_ok("./autogen.sh", src)?;
        } else {
            sys::run_ok("autoreconf", &["-fiv"], src)?;
        }
    }
    let mut v: Vec<String> = vec![
        "./configure".into(),
        format!("--prefix={}", e.prefix),
        format!("--libdir={}/lib", e.prefix),
        "--disable-shared".into(),
        "--enable-static".into(),
        "--with-pic".into(),
    ];
    v.extend(args.iter().map(|s| s.to_string()));
    // configure 是 shell 脚本——走 bash 保证 shebang/路径兼容
    // configure is a shell script — run it via bash for shebang/path parity
    sys::bash_ok(
        &v.iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" "),
        src,
    )?;
    let mut mk: Vec<String> = vec!["-j".into(), e.jobs.to_string()];
    mk.extend(mvars.iter().map(|s| s.to_string()));
    sys::run_ok(
        "make",
        &mk.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        src,
    )?;
    let mut mi: Vec<&str> = vec!["install"];
    mi.extend(mvars.iter().copied());
    sys::run_ok("make", &mi, src)
}

/// meson_build [args]：meson 标准配方构建（静态、关 LTO）
/// meson_build [args]: standard meson recipe build (static, LTO off)
pub fn meson_build(src: &Path, args: &[&str]) -> Result<(), String> {
    let e = sys::env();
    // -Db_lto=false：gcc LTO 字节码锁版本，依赖归档整体链进 ffmpeg 无用
    // gcc LTO bytecode is version-locked; dep-side LTO buys nothing
    let mut v: Vec<String> = vec![
        "meson".into(),
        "setup".into(),
        "build".into(),
        format!("--prefix={}", e.prefix),
        "--libdir=lib".into(),
        "--buildtype=release".into(),
        "--default-library=static".into(),
        "-Db_lto=false".into(),
    ];
    // -liconv 进 c_link_args/cpp_link_args：meson 把用户 link args
    // 放进 --start-group 组内（实证组内重扫即解改名符号）；env
    // LDFLAGS 必须并入——-Dc_link_args 顶替 LDFLAGS 的 env 初值，不
    // 并入则 -L 前缀路径全丢、-liconv 也无从定位 / -liconv into
    // c_link_args/cpp_link_args: meson places user link args inside
    // --start-group (empirically the group rescan resolves renamed
    // symbols); env LDFLAGS must be folded in — -Dc_link_args replaces
    // LDFLAGS' env-derived initial value, and without the fold the -L
    // prefix path drops and -liconv goes unlocatable too
    if iconv_tail() {
        let ld = std::env::var("LDFLAGS").unwrap_or_default();
        v.push(format!("-Dc_link_args={ld} -liconv"));
        v.push(format!("-Dcpp_link_args={ld} -liconv"));
    }
    v.extend(args.iter().map(|s| s.to_string()));
    sys::bash_ok(
        &v.iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" "),
        src,
    )?;
    sys::run_ok(
        "meson",
        &["compile", "-C", "build", "-j", &e.jobs.to_string()],
        src,
    )?;
    sys::run_ok("meson", &["install", "-C", "build"], src)
}

/// shell_quote：把单个参数包成 bash 单引号串 / quote one arg for bash
pub fn shell_quote(a: &str) -> String {
    if a.chars()
        .all(|c| c.is_ascii_alphanumeric() || "@%_+-=:,./".contains(c))
    {
        return a.to_string();
    }
    format!("'{}'", a.replace('\'', "'\\''"))
}
