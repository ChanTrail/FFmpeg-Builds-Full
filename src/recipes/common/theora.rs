//! theora 配方：Theora 视频（依赖 ogg）
//! theora recipe: Theora video (depends on ogg)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "theora",
        "linux,mac",
        "theoraenc",
        "--enable-libtheora",
        body,
    );
    x.src = include_str!("theora.rs");
    x
}

/// 构建 libtheora / Build libtheora
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &["--disable-examples", "--disable-spec"])
}
