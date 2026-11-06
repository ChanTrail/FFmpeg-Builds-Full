//! librist 配方：RIST 传输协议（钉 0.2.20；旧 60-media-io.sh 移植）
//! librist recipe: RIST transport protocol, pinned 0.2.20 (port of
//! 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("librist", "mac", "librist", "--enable-librist", body);
    x.src = include_str!("librist.rs");
    x
}

/// 构建 librist / Build librist
fn body(ctx: &Ctx) -> Result<(), String> {
    // 上游删掉了 'test_mcast' 选项；meson 遇未知选项中止
    // upstream removed the 'test_mcast' option; meson aborts on unknowns
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &["-Duse_mbedtls=false", "-Dbuilt_tools=false", "-Dtest=false"],
    )
}
