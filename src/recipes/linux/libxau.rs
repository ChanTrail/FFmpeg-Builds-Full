//! libxau 配方：X 授权库（libxcb 的 XauGetAuthByAddr 依赖）
//! libxau recipe: X authorization library (libxcb wants
//! XauGetAuthByAddr)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libxau", "linux", "xau", "-", body);
    x.src = include_str!("libxau.rs");
    x
}

/// 构建 libXau 静态库 / Build libXau statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
