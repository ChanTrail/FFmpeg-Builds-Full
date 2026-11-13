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
            // test/checkasm 都是 feature 型——只能 enabled/disabled/
            // auto，写 false 会报 "not one of the choices"；compare/
            // profile/fuzz 类型拿不准，默认即关，不钉 / test and
            // checkasm are feature-typed — only enabled/disabled/auto
            // accepted, "false" is rejected outright; compare/profile/
            // fuzz default to off anyway so leave them unpinned
            "-Dtest=disabled",
            "-Dcheckasm=disabled",
        ],
    )
}
