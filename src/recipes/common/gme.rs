//! gme 配方：游戏音乐模拟器
//! gme recipe: the game music emulator

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("gme", "linux,mac", "libgme", "--enable-libgme", body);
    x.src = include_str!("gme.rs");
    x
}

/// 构建 libgme / Build libgme
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DBUILD_SHARED_LIBS=OFF", "-DENABLE_UBSAN=OFF"])
}
