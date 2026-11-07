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
    let mut x = r("whisper", "all", "whisper", "--enable-whisper", body);
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
    ];
    // GPU 后端按平台选：linux/windows 走 Vulkan（驱动 DLL/.so 只在请求
    // 设备时才加载）；macOS 没有干净的前缀 Vulkan 路——走 Metal，着色
    // 器编进库里免运行时依赖 / the GPU backend is platform-picked:
    // Vulkan on linux/windows (the driver only loads when a device is
    // actually requested); macOS has no clean prefix-Vulkan path — Metal
    // instead, shaders embedded so nothing is needed at runtime
    match e.os {
        sys::Os::Mac => args.extend(
            [
                "-DGGML_VULKAN=OFF",
                "-DGGML_METAL=ON",
                "-DGGML_METAL_EMBED_LIBRARY=ON",
            ]
            .iter()
            .map(|s| s.to_string()),
        ),
        _ => args.push("-DGGML_VULKAN=ON".into()),
    }
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
    // ggml 后端库按平台拼：vulkan 后端链 -lggml-vulkan 并经
    // Requires.private 挂 vulkan.pc；Metal 后端链 -lggml-metal 加
    // Metal/Foundation framework / the ggml backend lib is
    // platform-picked: -lggml-vulkan plus Requires.private vulkan.pc on
    // one side, -lggml-metal plus Metal/Foundation frameworks on macOS
    if e.os == sys::Os::Mac {
        pkgcfg::libs_private(
            "whisper",
            &[
                "-lggml",
                "-lggml-cpu",
                "-lggml-base",
                "-lggml-metal",
                &e.cxxrt,
                "-lm",
                "-lpthread",
                "-framework Metal",
                "-framework Foundation",
                "-framework CoreFoundation",
            ],
        );
    } else {
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
        // 加载器经 Requires.private 挂上（windows shim / linux vendored
        // shim 都发 vulkan.pc）/ hook the loader via Requires.private
        // (both shims ship vulkan.pc)
        pkgcfg::requires_private("whisper", &["vulkan"]);
    }
    Ok(())
}
