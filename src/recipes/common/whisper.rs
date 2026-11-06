//! whisper 配方：whisper.cpp 语音转写（CPU + Vulkan 后端）
//! whisper recipe: whisper.cpp speech transcription (CPU + Vulkan
//! backends)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "whisper",
        "linux,windows",
        "whisper",
        "--enable-whisper",
        body,
    );
    x.src = include_str!("whisper.rs");
    x
}

/// 构建 whisper.cpp / Build whisper.cpp
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let mut args: Vec<String> = vec![
        "-DWHISPER_BUILD_TESTS=OFF".into(),
        "-DWHISPER_BUILD_EXAMPLES=OFF".into(),
        "-DWHISPER_BUILD_SERVER=OFF".into(),
        "-DWHISPER_USE_SYSTEM_GGML=OFF".into(),
        "-DGGML_CCACHE=OFF".into(),
        "-DGGML_OPENMP=OFF".into(),
        "-DGGML_VULKAN=ON".into(),
    ];
    // Vulkan 后端是唯一跨平台 GPU 路：驱动 DLL/.so 只在用到设备时才加载
    // the Vulkan backend is the only cross-platform GPU path: the driver
    // is only loaded when a device is actually requested
    match e.arch {
        sys::Arch::X86_64 => args.extend(
            [
                "-DGGML_NATIVE=OFF",
                "-DGGML_SSE42=ON",
                "-DGGML_AVX=ON",
                "-DGGML_F16C=ON",
                "-DGGML_AVX2=ON",
                "-DGGML_BMI2=ON",
                "-DGGML_FMA=ON",
            ]
            .iter()
            .map(|s| s.to_string()),
        ),
        _ => args.push("-DGGML_NATIVE=OFF".into()),
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    buildsys::cmake_build(&src, &refs)?;
    let pc = e.prefix_win.join("lib/pkgconfig/whisper.pc");
    // 链接顺序：whisper 在前，然后是它需要的 ggml 部件 / link order:
    // whisper first, then the ggml pieces it needs
    fsx::sed_i("s/^\\(Libs:\\).*$/\\1 -L${libdir} -lwhisper/", &pc, &e.work)?;
    pkgcfg::libs_private(
        "whisper",
        &[
            "-lggml",
            "-lggml-cpu",
            "-lggml-base",
            "-lggml-vulkan",
            &e.cxxrt,
            "-lm",
            "-lpthread",
        ],
    );
    // 加载器经 Requires.private 挂上（windows shim / linux vendored shim
    // 都发 vulkan.pc）/ hook the loader via Requires.private (both shims
    // ship vulkan.pc)
    pkgcfg::requires_private("whisper", &["vulkan"]);
    Ok(())
}
