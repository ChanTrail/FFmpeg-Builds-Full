//! vvenc 配方：VVC 编码器
//! vvenc recipe: VVC encoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys::{self, Arch};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("vvenc", "all", "libvvenc", "--enable-libvvenc", body);
    x.src = include_str!("vvenc.rs");
    x
}

/// 构建 vvenc / Build vvenc
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let mut args = vec![
        "-DVVENC_LIBRARY_ONLY=ON".to_string(),
        "-DVVENC_ENABLE_WERROR=OFF".to_string(),
        "-DVVENC_ENABLE_LINK_TIME_OPT=OFF".to_string(),
        // vvenc 找到 ccache 就无条件包编译；MSYS2 的 ccache 解析不了 PATH
        // 里的绝对编译器路径。预设结果变量让 find 被跳过 / vvenc wraps
        // compiles in ccache whenever found (no opt-out); MSYS2's ccache
        // fails to resolve the absolute compiler path. Pre-set the result
        // var so the find is skipped
        "-DCCACHE_FOUND=FALSE".to_string(),
        format!("-DEXTRALIBS={}", e.cxxrt),
    ];
    if e.arch == Arch::Arm64 {
        args.push("-DVVENC_ENABLE_ARM_SIMD=ON".to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    buildsys::cmake_build(&src, &refs)
}
