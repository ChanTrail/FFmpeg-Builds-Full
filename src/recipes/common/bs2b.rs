//! bs2b 配方：Bauer 立体声-双耳滤波
//! bs2b recipe: Bauer stereo-to-binaural filter

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("bs2b", "linux,mac", "libbs2b", "--enable-libbs2b", body);
    x.src = include_str!("bs2b.rs");
    x
}

/// 构建 libbs2b / Build libbs2b
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    buildsys::refresh_config_scripts(&src)?; // 自带的早于 aarch64 / predates aarch64
    sys::bash_ok(
        &format!(
            "./configure --prefix={} --libdir={}/lib --disable-shared --enable-static --with-pic",
            e.prefix, e.prefix
        ),
        &src,
    )?;
    // 只要库：演示工具把 LIBS（-lm）排在 libbs2b.a 之前，pow/exp 解析失败
    // only the library: the demo tools order LIBS (-lm) before libbs2b.a
    fsx::sed_i(
        "s/^bin_PROGRAMS *=.*/bin_PROGRAMS =/",
        &src.join("src/Makefile"),
        &src,
    )?;
    let sm = src.join("src");
    sys::run_ok("make", &["-j", &e.jobs.to_string()], &sm)?;
    sys::run_ok("make", &["install"], &sm)?;
    fsx::install(
        &src.join("libbs2b.pc"),
        &e.prefix_win.join("lib/pkgconfig/libbs2b.pc"),
    )
}
