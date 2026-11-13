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
            // Xcode 26 SDK 给 AURenderCallback typedef 加了 nonblocking
            // 函数属性——coreaudio.cpp 的 noexcept lambda 转换被新版
            // clang 按 -Wfunction-effects 拦死。upstream 在
            // ALSOFT_WERROR=OFF 时仍逐条追加 -Werror=function-effects，
            // 警告集本身又是 check_cxx_compiler_flag 探出来的——预置
            // 缓存变量让探测直接跳过，两条旗标都进不了编译行 / the
            // Xcode 26 SDK added `nonblocking` to the AURenderCallback
            // typedef — new clang rejects the noexcept lambda
            // conversion under -Wfunction-effects. Upstream appends
            // -Werror=function-effects even with ALSOFT_WERROR=OFF, and
            // the warning set itself comes from
            // check_cxx_compiler_flag — pre-seed the cache var so the
            // probe is skipped and neither flag reaches the compile line
            "-DHAVE_WFUNCTION_EFFECTS=FALSE",
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
