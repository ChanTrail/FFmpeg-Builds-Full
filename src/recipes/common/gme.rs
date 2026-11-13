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
    // CMAKE_DISABLE_FIND_PACKAGE_SDL2：gme 只要库——player demo 会
    // find_package(SDL2)，而 SDL2 的导出 target 引用了 env 里没有的
    // ALSA::ALSA（apt 的 libasound2-dev 让 SDL2 探到了 ALSA），
    // generate 阶段必炸 / CMAKE_DISABLE_FIND_PACKAGE_SDL2: we only
    // want the library — the player demo calls find_package(SDL2) and
    // SDL2's exported target references ALSA::ALSA (apt's
    // libasound2-dev let SDL2 detect ALSA) which nothing defines, so
    // the generate step always dies
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_SHARED_LIBS=OFF",
            "-DENABLE_UBSAN=OFF",
            "-DCMAKE_DISABLE_FIND_PACKAGE_SDL2=ON",
        ],
    )
}
