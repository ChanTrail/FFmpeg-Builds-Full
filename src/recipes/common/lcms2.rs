//! lcms2 配方：LittleCMS 色彩管理
//! lcms2 recipe: LittleCMS color management

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("lcms2", "linux,mac", "lcms2", "--enable-lcms2", body);
    x.src = include_str!("lcms2.rs");
    x
}

/// 构建 lcms2 / Build lcms2
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &["-Djpeg=disabled", "-Dtiff=disabled", "-Dutils=disabled"],
    )
}
