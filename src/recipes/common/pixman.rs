//! pixman 配方：cairo 的像素后端
//! pixman recipe: cairo's pixel backend

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("pixman", "linux,mac", "pixman-1", "-", body);
    x.src = include_str!("pixman.rs");
    x
}

/// 构建 pixman / Build pixman
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &[
            "-Dtests=disabled",
            "-Ddemos=disabled",
            "-Dgtk=disabled",
            "-Dlibpng=disabled",
            "-Dopenmp=disabled",
            "-Dgnuplot=false",
        ],
    )
}
