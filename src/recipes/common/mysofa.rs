//! mysofa 配方：SOFA/HRTF 空间音频
//! mysofa recipe: SOFA/HRTF spatial audio

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "mysofa",
        "linux,mac",
        "libmysofa",
        "--enable-libmysofa",
        body,
    );
    x.src = include_str!("mysofa.rs");
    x
}

/// 构建 libmysofa / Build libmysofa
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_TESTS=OFF",
            "-DBUILD_STATIC_LIBS=ON",
            "-DCODE_COVERAGE=OFF",
        ],
    )
}
