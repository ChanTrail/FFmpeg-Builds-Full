//! libdvdread 配方：DVD 读取（7.x 已去 autotools；libdvdcss 缺席时自
//! 动探测为禁用）
//! libdvdread recipe: DVD reading, 7.x dropped autotools and libdvdcss
//! auto-detects to disabled when absent

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libdvdread",
        "linux,mac",
        "dvdread",
        "--enable-libdvdread",
        body,
    );
    x.src = include_str!("libdvdread.rs");
    x
}

/// 构建 libdvdread / Build libdvdread
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(&src, &[])
}
