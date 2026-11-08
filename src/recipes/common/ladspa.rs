//! ladspa 配方：LADSPA 插件 API 头文件——FFmpeg 的 ladspa filter 运行时
//! dlopen 插件，编时只需要 ladspa.h 在场（sources/misc.yaml 各平台都有
//! ladspa_sdk 包，但只有 macos-arm64 的 misc 分节启用了这个特性）
//! ladspa recipe: the LADSPA plugin API header — FFmpeg's ladspa filter
//! dlopens plugins at runtime, so ladspa.h is all the compile needs
//! (sources/misc.yaml carries the ladspa_sdk tarball for every platform,
//! but only macos-arm64's misc section enables the feature)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "ladspa",
        "mac",
        "file:include/ladspa.h",
        "--enable-ladspa",
        body,
    );
    x.src = include_str!("ladspa.rs");
    x
}

/// 只装 ladspa.h——SDK 的 .so/插件/工具一概不要
/// Install ladspa.h only — none of the SDK's .so/plugins/tools
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // ladspa_sdk tarball 的目录形状随版本飘——按文件名找头文件，不赌
    // 固定相对路径 / the ladspa_sdk tarball's layout drifts between
    // releases — match the header by file name rather than betting on
    // a fixed relative path
    let hdr = fsx::walk(&src)
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == "ladspa.h"))
        .ok_or("ladspa: ladspa.h not found in the ladspa_sdk tarball")?;
    fsx::install(&hdr, &e.prefix_win.join("include").join("ladspa.h"))
}
