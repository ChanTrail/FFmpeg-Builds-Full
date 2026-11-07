//! libpng 配方：PNG 读写（aribb24 依赖；cairo/leptonica 也会探测）
//! libpng recipe: PNG I/O (aribb24 dependency; cairo/leptonica probe it
//! too)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libpng", "mac", "libpng", "-", body);
    x.src = include_str!("libpng.rs");
    x
}

/// 构建 libpng / Build libpng
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DPNG_SHARED=OFF",
            "-DPNG_STATIC=ON",
            "-DPNG_TOOLS=OFF",
            "-DPNG_TESTS=OFF",
        ],
    )?;
    // cmake 安装给的是 libpng16.pc——pkg-config 消费者按 "libpng" 名查，
    // 补一份别名 / cmake installs libpng16.pc — pkg-config consumers query
    // the "libpng" name, so alias a copy
    let pcdir = e.prefix_win.join("lib/pkgconfig");
    let (a, b) = (pcdir.join("libpng16.pc"), pcdir.join("libpng.pc"));
    if a.is_file() && !b.is_file() {
        fsx::write(&b, &fsx::read(&a).replace("libpng16", "libpng"))?;
    }
    Ok(())
}
