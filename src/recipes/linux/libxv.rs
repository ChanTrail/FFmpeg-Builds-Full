//! libxv 配方：X Video 扩展（xlib 特性的第三个检查件 Xvlib.h）
//! libxv recipe: X Video extension (the xlib feature's third piece,
//! Xvlib.h)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libxv", "linux", "xv", "-", body);
    x.src = include_str!("libxv.rs");
    x
}

/// 构建 libXv 静态库 / Build libXv statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
