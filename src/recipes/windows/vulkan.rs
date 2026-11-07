//! vulkan 配方：Vulkan 头文件 + BtbN 的 Vulkan-Shim-Loader（shim 冒充
//! 真加载器 libvulkan-1.a + vulkan.pc，实际是小静态库：真请求设备时才
//! LoadLibrary vulkan-1.dll）
//! vulkan recipe: Vulkan headers + BtbN's Vulkan-Shim-Loader (the shim
//! impersonates the real loader — libvulkan-1.a + vulkan.pc — but is a
//! small static lib that only LoadLibrarys vulkan-1.dll when a device is
//! actually requested)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("vulkan", "windows", "vulkan", "--enable-vulkan", body);
    x.src = include_str!("vulkan.rs");
    x
}

/// 构建 Vulkan 头文件 + shim 加载器
/// Build the Vulkan headers + shim loader
fn body(ctx: &Ctx) -> Result<(), String> {
    // 第一段：Vulkan-Headers / part one: Vulkan-Headers
    let hdrs = ctx.src_named("vulkan-headers")?;
    buildsys::cmake_build(
        &hdrs,
        &[
            "-DVULKAN_HEADERS_ENABLE_MODULE=NO",
            "-DVULKAN_HEADERS_ENABLE_TESTS=NO",
            "-DVULKAN_HEADERS_ENABLE_INSTALL=YES",
        ],
    )?;
    // fetch 到的 src 目录挪开，让位给 shim 检出 / move the fetched src
    // aside for the shim checkout
    let headers = ctx.wd.join("headers");
    std::fs::rename(&hdrs, &headers).map_err(|e| format!("mv src headers: {e}"))?;
    // 第二段：Shim-Loader——它的 CMakeLists 会 add_subdirectory 一个
    // Vulkan-Headers 检出；git 把子模块路径物化成空目录，先删掉再把我们
    // 的头文件挪进去 / the shim's CMakeLists add_subdirectory()s a
    // Vulkan-Headers checkout; git materializes the submodule path as an
    // empty dir — remove it, then drop our headers in
    let shim = ctx.src_named("vulkan-shim")?;
    fsx::rm_rf(&shim.join("Vulkan-Headers"));
    std::fs::rename(&headers, shim.join("Vulkan-Headers"))
        .map_err(|e| format!("mv headers into shim: {e}"))?;
    buildsys::cmake_build(&shim, &["-DVULKAN_SHIM_IMPERSONATE=ON"])
}
