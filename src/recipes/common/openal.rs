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
            // GCC 的 -fmodules-ts 在 alconfig.cpp 上 ICE（GIMPLE
            // nonnull_arg_p）——关掉 C++20 模块扫描 /
            // GCC's -fmodules-ts ICEs on alconfig.cpp (GIMPLE
            // nonnull_arg_p) — disable C++20 module scanning
            "-DCMAKE_CXX_SCAN_FOR_MODULES=OFF",
        ],
    )
}
