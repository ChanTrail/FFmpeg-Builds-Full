//! dav1d 配方：AV1 解码器
//! dav1d recipe: the AV1 decoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("dav1d", "linux,mac", "dav1d", "--enable-libdav1d", body);
    x.src = include_str!("dav1d.rs");
    x
}

/// 构建 dav1d / Build dav1d
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &[
            "-Denable_tools=false",
            "-Denable_tests=false",
            "-Denable_examples=false",
        ],
    )
}
