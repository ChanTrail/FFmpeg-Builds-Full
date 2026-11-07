//! libass 配方：ASS/SSA 字幕渲染（freetype/fribidi/harfbuzz/fontconfig 全套前缀供给）
//! libass recipe: ASS/SSA subtitle rendering (freetype/fribidi/harfbuzz/fontconfig all prefix-supplied)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libass", "linux,mac", "libass", "--enable-libass", body);
    x.src = include_str!("libass.rs");
    x
}

/// 构建 libass / Build libass
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &[
            "-Dfontconfig=enabled",
            "-Dtest=false",
            "-Dcompare=false",
            "-Dprofile=false",
            "-Dfuzz=false",
            "-Dcheckasm=disabled",
        ],
    )
}
