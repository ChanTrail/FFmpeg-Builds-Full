//! libxfixes 配方：X Fixes 扩展（libva 的 libva-x11 后端要求）
//! libxfixes recipe: X Fixes extension (required by libva's libva-x11
//! backend)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libxfixes", "linux", "xfixes", "-", body);
    x.src = include_str!("libxfixes.rs");
    x
}

/// 构建 libXfixes 静态库 / Build libXfixes statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
