//! sdl2 配方：SDL2（ffplay/sdl 输出设备用）
//! sdl2 recipe: SDL2 (ffplay/sdl output devices)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("sdl2", "linux,mac", "sdl2", "--enable-sdl2", body);
    x.src = include_str!("sdl2.rs");
    x
}

/// 构建 SDL2 / Build SDL2
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DSDL_SHARED=OFF",
            "-DSDL_STATIC=ON",
            "-DSDL_TEST=OFF",
            "-DSDL_TESTS=OFF",
        ],
    )
}
