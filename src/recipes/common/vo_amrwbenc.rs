//! vo-amrwbenc 配方：AMR-WB 编码
//! vo-amrwbenc recipe: AMR-WB encoding

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "vo-amrwbenc",
        "linux,mac",
        "vo-amrwbenc",
        "--enable-libvo-amrwbenc",
        body,
    );
    x.src = include_str!("vo_amrwbenc.rs");
    x
}

/// 构建 vo-amrwbenc / Build vo-amrwbenc
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &["--disable-examples"])
}
