//! gsm 配方：GSM 语音编解码（手写 Makefile，无构建系统）
//! gsm recipe: the GSM speech codec (hand-written Makefile, no build
//! system)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "gsm",
        "linux,mac",
        "file:lib/libgsm.a",
        "--enable-libgsm",
        body,
    );
    x.src = include_str!("gsm.rs");
    x
}

/// 构建 libgsm / Build libgsm
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 上游 Makefile 只认自己的 flags；K&R 代码需要 gnu89
    // the upstream Makefile only knows its own flags; K&R code needs gnu89
    sys::run_ok(
        "make",
        &[
            "-j1",
            "lib/libgsm.a",
            &format!("CC={} -std=gnu89", e.cc),
            "CCFLAGS=-c -O2 -DNeedFunctionPrototypes=1 -DSASR -fPIC -Wno-error",
        ],
        &src,
    )?;
    fsx::install(
        &src.join("inc/gsm.h"),
        &e.prefix_win.join("include/gsm/gsm.h"),
    )?;
    fsx::install(
        &src.join("lib/libgsm.a"),
        &e.prefix_win.join("lib/libgsm.a"),
    )
}
