//! quirc 配方：手工编译静态库（无构建系统；旧 40-imaging.sh 移植）
//! quirc recipe: hand-compiled static lib, no build system (port of
//! 40-imaging.sh)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "quirc",
        "all",
        "file:lib/libquirc.a",
        "--enable-libquirc",
        body,
    );
    x.src = include_str!("quirc.rs");
    x
}

/// 构建 libquirc / Build libquirc
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let cflags = std::env::var("CFLAGS").unwrap_or_default();
    let mut objs: Vec<String> = Vec::new();
    let lib = src.join("lib");
    for f in fsx::walk(&lib) {
        if f.extension().map(|x| x == "c").unwrap_or(false) {
            let o = f.with_extension("o");
            let mut args: Vec<String> = cflags.split_whitespace().map(String::from).collect();
            args.extend(
                [
                    "-O3",
                    "-fPIC",
                    "-Ilib",
                    "-c",
                    &f.to_string_lossy(),
                    "-o",
                    &o.to_string_lossy(),
                ]
                .iter()
                .map(|s| s.to_string()),
            );
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            sys::run_ok(&e.cc, &refs, &src)?;
            objs.push(o.to_string_lossy().to_string());
        }
    }
    let mut a = vec!["rcs".to_string(), "libquirc.a".to_string()];
    a.extend(objs);
    let refs: Vec<&str> = a.iter().map(String::as_str).collect();
    sys::run_ok(&e.ar, &refs, &src)?;
    sys::run_ok(&e.ranlib, &["libquirc.a"], &src)?;
    fsx::install(
        &src.join("libquirc.a"),
        &e.prefix_win.join("lib/libquirc.a"),
    )?;
    fsx::install(
        &src.join("lib/quirc.h"),
        &e.prefix_win.join("include/quirc.h"),
    )
}
