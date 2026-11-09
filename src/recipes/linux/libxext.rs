//! libxext 配方：X 扩展库（Xext 协议 + xlib 特性的第二个检查件）
//! libxext recipe: X extensions library (Xext protocol + the second
//! piece of the xlib feature)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libxext", "linux", "xext", "-", body);
    x.src = include_str!("libxext.rs");
    x
}

/// 构建 libXext 静态库 / Build libXext statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
