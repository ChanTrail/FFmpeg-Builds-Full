//! glslang 配方：Khronos GLSL→SPIR-V 编译器（libplacebo vulkan 后端
//! 的运行时着色器编译器——无它 vulkan 后端"very limited in
//! functionality"，libplacebo meson 原话）。ENABLE_OPT=OFF 砍掉
//! SPIRV-Tools 优化器依赖（还牵 abseil），libplacebo 只要编译不要
//! 优化。windows 走 pacman shaderc 故不适用。
//! glslang recipe: the Khronos GLSL→SPIR-V compiler (the runtime shader
//! compiler of libplacebo's vulkan backend — without it the vulkan
//! backend is "very limited in functionality", per libplacebo's own
//! meson warning). ENABLE_OPT=OFF drops the SPIRV-Tools optimizer
//! dependency (which would drag abseil in) — libplacebo needs
//! compilation, not optimization. Windows uses pacman's shaderc, so
//! this recipe does not apply there.

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "glslang",
        "linux,mac",
        "file:lib/libglslang.a,file:lib/libSPIRV.a,hdrsym:glslang/build_info.h@GLSLANG_VERSION_MAJOR",
        "-",
        body,
    );
    x.src = include_str!("glslang.rs");
    x
}

/// 构建 glslang / Build glslang
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_SHARED_LIBS=OFF",
            // ENABLE_OPT=OFF：优化器要 SPIRV-Tools（现代版还牵 abseil
            // ），libplacebo 用 glslang 仅做 GLSL→SPIR-V 编译，优化
            // 是可选路径 / ENABLE_OPT=OFF: the optimizer wants
            // SPIRV-Tools (modern versions drag in abseil too) —
            // libplacebo uses glslang purely for GLSL→SPIR-V
            // compilation; optimization is an optional path
            "-DENABLE_OPT=OFF",
            "-DENABLE_GLSLANG_BINARIES=OFF",
            "-DENABLE_CTEST=OFF",
            "-DBUILD_TESTING=OFF",
            "-DENABLE_PCH=OFF",
        ],
    )
}
