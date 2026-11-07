//! svtav1 配方：SVT-AV1 编码器
//! svtav1 recipe: the SVT-AV1 encoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "svtav1",
        "linux,mac",
        "SvtAv1Enc",
        "--enable-libsvtav1",
        body,
    );
    x.src = include_str!("svtav1.rs");
    x
}

/// 构建 SVT-AV1 / Build SVT-AV1
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DBUILD_APPS=OFF", "-DBUILD_TESTING=OFF"])
}
