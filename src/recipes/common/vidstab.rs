//! vidstab 配方：vid.stab 视频稳定化
//! vidstab recipe: vid.stab video stabilization

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "vidstab",
        "linux,mac",
        "vidstab",
        "--enable-libvidstab",
        body,
    );
    x.src = include_str!("vidstab.rs");
    x
}

/// 构建 vid.stab / Build vid.stab
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DUSE_OMP=OFF"])
}
