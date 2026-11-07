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
        // jpeg/tiff 是 feature 型（disabled），utils 是 bool 型
        // （false）——类型写混 meson 直接报错 / jpeg/tiff are
        // feature-typed (disabled) while utils is a bool (false) —
        // mixing the two fails meson's type check
        &["-Djpeg=disabled", "-Dtiff=disabled", "-Dutils=false"],
    )
}
