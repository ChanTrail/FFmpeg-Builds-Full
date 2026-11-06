//! zstd 配方：仅 macOS（libcurl 的 --with-zstd 依赖栈补齐）
//! zstd recipe: macOS only (closes the libcurl --with-zstd dep-stack gap)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("zstd", "mac", "libzstd", "-", body);
    x.src = include_str!("zstd.rs");
    x
}

/// 构建 zstd / Build zstd
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // zstd 的 cmake 在 build/cmake 下；顶层没有 CMakeLists。LANGUAGES C
    // 声明在 CXX 环境变量被设过时会去摸 C++ 编译器（BtbN 配方同款修正）。
    // zstd's cmake lives under build/cmake; there is no top-level
    // CMakeLists. The LANGUAGES C declaration still touches the C++
    // compiler when a CXX env var was ever set (same fixup as BtbN).
    fsx::sed_i(
        "s/LANGUAGES C /LANGUAGES C CXX /",
        &src.join("build/cmake/CMakeLists.txt"),
        &src,
    )?;
    buildsys::cmake_build(
        &src.join("build/cmake"),
        &[
            "-DZSTD_BUILD_SHARED=OFF",
            "-DZSTD_BUILD_STATIC=ON",
            "-DZSTD_BUILD_PROGRAMS=OFF",
            "-DZSTD_BUILD_TESTS=OFF",
            "-DZSTD_BUILD_CONTRIB=OFF",
            "-DZSTD_LEGACY_SUPPORT=OFF",
        ],
    )
}
