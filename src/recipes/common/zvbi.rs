//! zvbi 配方：图文电视/VBI 解码
//! zvbi recipe: teletext/VBI decoding

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("zvbi", "linux,mac", "zvbi-0.2", "--enable-libzvbi", body);
    x.src = include_str!("zvbi.rs");
    x
}

/// 构建 libzvbi / Build libzvbi
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(
        &src,
        &[
            "--disable-dvb",
            "--disable-bktr",
            "--disable-nls",
            "--without-doxygen",
        ],
    )
}
