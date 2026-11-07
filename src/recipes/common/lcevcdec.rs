//! lcevcdec 配方：LCEVC 解码器
//! lcevcdec recipe: LCEVC decoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys::{self, Os};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "lcevcdec",
        "linux,mac,windows:x86_64",
        "lcevc_dec",
        "--enable-liblcevc-dec",
        body,
    );
    x.src = include_str!("lcevcdec.rs");
    x
}

/// 构建 LCEVCdec / Build LCEVCdec
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 其 cmake 在 Darwin 上通过 shell 调 `libtool -static` 合并归档集——
    // PATH 里靠前的 GNU libtool 会拒绝该旗标。仅对此配方钉死真 Apple
    // libtool / its cmake shells out to `libtool -static` on Darwin — a
    // GNU libtool earlier in PATH rejects the flag; pin the real Apple
    // libtool for this recipe only
    let _g = if e.os == Os::Mac {
        let path = std::env::var("PATH").unwrap_or_default();
        Some(sys::EnvGuard::set(&[("PATH", format!("/usr/bin:{path}"))]))
    } else {
        None
    };
    buildsys::cmake_build(
        &src,
        &[
            "-DVN_SDK_EXECUTABLES=OFF",
            "-DVN_SDK_SAMPLE_SOURCE=OFF",
            "-DVN_SDK_TRACING=OFF",
            "-DVN_SDK_METRICS=OFF",
            "-DVN_SDK_SYSTEM_INSTALL=ON",
            "-DVN_SDK_PIPELINE_LEGACY=OFF",
            "-DVN_SDK_PIPELINE_VULKAN=OFF",
            &format!("-DPC_LIBS_PRIVATE=Libs.private: {}", e.cxxrt),
        ],
    )
}
