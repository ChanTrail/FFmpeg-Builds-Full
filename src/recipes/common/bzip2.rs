//! bzip2 配方：libbz2 静态库
//! bzip2 recipe: libbz2 as a static library

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "bzip2",
        "linux",
        "file:lib/libbz2.a",
        "--enable-bzlib",
        body,
    );
    x.src = include_str!("bzip2.rs");
    x
}

/// 构建 libbz2 静态库 / Build bzip2 as a static library
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let cflags = std::env::var("CFLAGS").unwrap_or_default();
    sys::run_ok(
        "make",
        &[
            "-j",
            &e.jobs.to_string(),
            "libbz2.a",
            &format!("CFLAGS={cflags} -O2 -fPIC -D_FILE_OFFSET_BITS=64"),
        ],
        &src,
    )?;
    fsx::install(
        &src.join("libbz2.a"),
        &e.prefix_win.join("lib").join("libbz2.a"),
    )?;
    fsx::install(
        &src.join("bzlib.h"),
        &e.prefix_win.join("include").join("bzlib.h"),
    )
}
