//! twolame 配方：MPEG-1 Layer II 音频
//! twolame recipe: MPEG-1 Layer II audio

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "twolame",
        "linux,mac",
        "twolame",
        "--enable-libtwolame",
        body,
    );
    x.src = include_str!("twolame.rs");
    x
}

/// 构建 libtwolame / Build libtwolame
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &["--disable-sndfile"])
}
