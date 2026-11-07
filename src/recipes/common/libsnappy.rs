//! snappy 配方：Google 快速压缩（SVT-AV1/若干封装用）
//! snappy recipe: Google's fast compression (SVT-AV1/muxers use it)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libsnappy",
        "linux,mac",
        "snappy",
        "--enable-libsnappy",
        body,
    );
    x.src = include_str!("libsnappy.rs");
    x
}

/// 构建 snappy / Build snappy
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &["-DSNAPPY_BUILD_TESTS=OFF", "-DSNAPPY_BUILD_BENCHMARKS=OFF"],
    )
}
