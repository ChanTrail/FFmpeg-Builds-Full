//! 构建系统封装：autotools/cmake/meson 的标准静态构建
//! Build-system wrappers: standard static builds for autotools/cmake/meson

use crate::fsx;
use crate::sys;
use std::path::Path;

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
    let e = sys::env();
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
    sys::run_ok("make", &["-j", &e.jobs.to_string()], src)?;
    sys::run_ok("make", &["install"], src)
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
