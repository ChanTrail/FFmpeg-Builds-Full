//! amf 配方：AMD AMF 硬件编解码头文件——FFmpeg 只要 include/AMF/**，
//! 实现由运行时 amfrt 库提供（x86_64 专属）
//! amf recipe: AMD AMF codec headers — FFmpeg wants only include/AMF/**;
//! the implementation ships in the runtime amfrt library (x86_64 only)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "amf",
        "linux:x86_64,windows:x86_64",
        "file:include/AMF/core/Factory.h",
        "--enable-amf",
        body,
    );
    x.src = include_str!("amf.rs");
    x
}

/// 安装 AMF 头文件 / Install the AMF headers
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 上游改过一次布局：新版 amf/public/include/ 直下就是
    // core|components（无 AMF/ 包裹层），旧版则是 include/AMF/**。
    // 两种都归一成 <prefix>/include/AMF/** / upstream reshuffled the
    // layout: newer trees carry core|components directly under
    // amf/public/include (no AMF/ wrapper), older ones used
    // include/AMF/** — normalise both into <prefix>/include/AMF/**
    let inner = src.join("amf/public/include");
    let payload = if inner.join("AMF").is_dir() {
        inner.join("AMF")
    } else {
        inner
    };
    let dst = e.prefix_win.join("include/AMF");
    fsx::rm_rf(&dst);
    fsx::install_d(&dst)?;
    sys::run_ok(
        "cp",
        &[
            "-R",
            &format!("{}/.", payload.display()),
            &dst.to_string_lossy(),
        ],
        &src,
    )
}
