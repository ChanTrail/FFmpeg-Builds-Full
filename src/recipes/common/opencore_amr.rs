//! opencore-amr 配方：AMR-NB/AMR-WB 编解码（单 tarball 一次出双库）
//! opencore-amr recipe: AMR-NB/AMR-WB (one tarball builds both libs)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "opencore-amr",
        "linux,mac",
        "opencore-amrnb,opencore-amrwb",
        "--enable-libopencore-amrnb --enable-libopencore-amrwb",
        body,
    );
    x.src = include_str!("opencore_amr.rs");
    x
}

/// 构建 opencore-amrnb + opencore-amrwb / Build both AMR libs
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 0.1.x tarball 是单一 autotools 工程，一次构建产出 amrnb 与 amrwb
    // 两套库和 .pc / the 0.1.x tarball is one autotools project producing
    // both amrnb and amrwb libs and .pc files in a single pass
    buildsys::autotools_build(&src, &[])
}
