//! zimg 配方：Z 图像缩放/色彩转换库
//! zimg recipe: the Z image scaling/colorspace library

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("zimg", "linux,mac", "zimg", "--enable-libzimg", body);
    x.src = include_str!("zimg.rs");
    x
}

/// 构建 zimg / Build zimg
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
