//! openal 配方：OpenAL 3D 音频
//! openal recipe: OpenAL 3D audio

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("openal", "linux,mac", "openal", "--enable-openal", body);
    x.src = include_str!("openal.rs");
    x
}

/// 构建 openal-soft / Build openal-soft
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DALSOFT_UTILS=OFF",
            "-DALSOFT_EXAMPLES=OFF",
            "-DALSOFT_TESTS=OFF",
            "-DALSOFT_INSTALL=ON",
            "-DLIBTYPE=STATIC",
            // C++20 模块整条关掉：GCC 16 + Ninja 自动启用（编译
            // gsl.cppm 等 .cppm），曾 ICE 也曾在 master 迁移期踩到
            // gsl 导出缺口；头文件路径才是久经考验的路 /
            // disable C++20 modules wholesale: GCC 16 + Ninja
            // auto-enables them (building gsl.cppm et al.), which has
            // both ICE'd before and tripped on a transient gsl export
            // gap on master; the header path is the proven one
            "-DALSOFT_ENABLE_MODULES=OFF",
            "-DCMAKE_CXX_SCAN_FOR_MODULES=OFF",
        ],
    )
}
