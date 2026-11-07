//! flite 配方：语音合成（大量上游修补）
//! flite recipe: speech synthesis (heavy upstream patching)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "flite",
        "all",
        "file:lib/libflite.a",
        "--enable-libflite",
        body,
    );
    x.src = include_str!("flite.rs");
    x
}

/// 构建 libflite* / Build libflite*
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // flite 自带远古 autotools 输入：config.sub 拒绝现代三元组
    // flite ships ancient autotools inputs: config.sub rejects modern
    // triplets
    buildsys::refresh_config_scripts(&src)?;
    let win = e.os == sys::Os::Windows;
    // cst_socket.c 只在 _MSC_VER 下才跳过 POSIX socket 头；MinGW 照样试
    // <sys/socket.h> 而死——放宽守卫 / only skips the POSIX socket headers
    // under _MSC_VER; MinGW still tries <sys/socket.h> — widen the guard
    if win {
        fsx::sed_i(
            "s/^#ifndef _MSC_VER$/#if !defined(_MSC_VER) \\&\\& !defined(_WIN32)/",
            &src.join("src/utils/cst_socket.c"),
            &src,
        )?;
    }
    // FFmpeg 只要归档和头文件——工具自身链接会死在重复/未定义符号上，
    // 清空工具的 ALL 变量并从 main 的 install 目标删二进制行
    // FFmpeg needs only archives+headers — tool links die on
    // duplicate/undefined symbols; empty tool ALL vars and drop the binary
    // lines from main's install target
    fsx::sed_i(
        "s/^ALL = \\$(BINDIR)\\/find_sts \\$(BINDIR)\\/flite_sort makescripts$/ALL =/",
        &src.join("tools/Makefile"),
        &src,
    )?;
    let main_mk = src.join("main/Makefile");
    fsx::sed_i(
        "/^ALL = shared_libs/,/each \\$(EXTRABINS)$/d",
        &main_mk,
        &src,
    )?;
    fsx::sed_i(
        "/basic binaries/,/flite_time.*INSTALLBINDIR/d",
        &main_mk,
        &src,
    )?;
    let cflags = format!("-O2 {}", std::env::var("CFLAGS").unwrap_or_default());
    let mut cfg = format!(
        "./configure --prefix={} --disable-shared --enable-static --with-audio=none",
        e.prefix
    );
    if win {
        let tri = match e.arch {
            sys::Arch::Arm64 => "aarch64",
            _ => "x86_64",
        };
        cfg.push_str(&format!(" --host={tri}-w64-mingw32"));
    }
    {
        let _g = sys::EnvGuard::set(&[("CFLAGS", cflags.clone())]);
        sys::bash_ok(&cfg, &src)?;
    }
    for f in fsx::grep_l(&src, &["cp -pd"]) {
        let _ = fsx::sed_i("s/cp -pd/cp -p/g", &f, &src);
    }
    // 把硬编码的 i386-mingw32-* 工具链名换成环境里的对应工具
    // replace hardcoded i386-mingw32-* tool names with the env's tools
    for f in fsx::grep_l(&src, &["i386-mingw32-"]) {
        let _ = fsx::sed_i(
            &format!(
                "s/i386-mingw32-gcc/{}/g; s/i386-mingw32-g++/{}/g; s/i386-mingw32-ar/{}/g; s/i386-mingw32-ranlib/{}/g; s/i386-mingw32-strip/{}/g; s/i386-mingw32-windres/windres/g",
                e.cc, e.cxx, e.ar, e.ranlib, e.strip
            ),
            &f,
            &src,
        );
    }
    {
        let _g = sys::EnvGuard::set(&[("CFLAGS", cflags.clone())]);
        sys::run_ok("make", &["-j1"], &src)?;
    }
    // Windows 上 flite 头把 c99_snprintf/ts_utf8_sequence_length 声明成
    // __inline，C99 inline 语义下任何 TU 都不发定义——只编译一次、注入
    // 每个归档 / on Windows flite's headers declare them __inline and no TU
    // emits a definition — compile once and inject into each archive
    if win {
        let shim = "#include <stdio.h>\n#include <stdarg.h>\nint c99_vsnprintf(char *str, size_t size, const char *format, va_list ap)\n{\n    return vsnprintf(str, size, format, ap);\n}\nint c99_snprintf(char *str, size_t size, const char *format, ...)\n{\n    int count;\n    va_list ap;\n    va_start(ap, format);\n    count = c99_vsnprintf(str, size, format, ap);\n    va_end(ap);\n    return count;\n}\nint ts_utf8_sequence_length(char c0)\n{\n    return (( 0xE5000000 >> (( c0 >> 3 ) & 0x1E )) & 3 ) + 1;\n}\n";
        let c = src.join("fliteshim.c");
        let o = src.join("fliteshim.o");
        fsx::write(&c, shim)?;
        // cflags 是整串，必须拆词后逐个传 / cflags is one string — split
        // it into separate argv entries
        let mut args: Vec<String> = cflags.split_whitespace().map(String::from).collect();
        args.extend(
            ["-c", "fliteshim.c", "-o", "fliteshim.o"]
                .into_iter()
                .map(String::from),
        );
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        sys::run_ok(&e.cc, &refs, &src)?;
        for f in fsx::walk(&src) {
            let n = f
                .file_name()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_default();
            if n.starts_with("libflite")
                && n.ends_with(".a")
                && f.parent().map(|p| p.ends_with("lib")).unwrap_or(false)
            {
                sys::run_ok(
                    &e.ar,
                    &["rcs", &f.to_string_lossy(), &o.to_string_lossy()],
                    &src,
                )?;
            }
        }
    }
    sys::run_ok("make", &["install"], &src)
}
