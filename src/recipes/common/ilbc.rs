//! ilbc 配方：iLBC 编解码（旧 30-audio.sh 移植）
//! ilbc recipe: the iLBC codec (port of 30-audio.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "ilbc",
        "linux,mac,windows",
        "file:lib/libilbc.a",
        "--enable-libilbc",
        body,
    );
    x.src = include_str!("ilbc.rs");
    x
}

/// 构建 libilbc / Build libilbc
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DBUILD_TESTING=OFF"])
}
