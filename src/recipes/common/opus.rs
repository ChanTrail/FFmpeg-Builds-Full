//! opus 配方：Opus 音频编解码
//! opus recipe: the Opus audio codec

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("opus", "linux,mac", "opus", "--enable-libopus", body);
    x.src = include_str!("opus.rs");
    x
}

/// 构建 opus / Build opus
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &[
            "-Ddocs=disabled",
            "-Dtests=disabled",
            "-Dextra-programs=disabled",
        ],
    )
}
