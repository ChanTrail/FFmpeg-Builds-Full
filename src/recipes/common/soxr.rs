//! soxr 配方：SoX 重采样（OpenMP 关闭保可移植）
//! soxr recipe: SoX resampling (OpenMP off for portability)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("soxr", "linux,mac", "soxr", "--enable-libsoxr", body);
    x.src = include_str!("soxr.rs");
    x
}

/// 构建 libsoxr / Build libsoxr
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // WITH_OPENMP=OFF：mac 没有 libomp，串行也够快 /
    // WITH_OPENMP=OFF: mac has no libomp; the serial path is fast enough
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_TESTS=OFF",
            "-DBUILD_EXAMPLES=OFF",
            "-DWITH_LSR_BINDINGS=OFF",
            "-DWITH_OPENMP=OFF",
        ],
    )
}
