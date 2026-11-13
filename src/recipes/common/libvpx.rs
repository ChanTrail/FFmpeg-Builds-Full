//! libvpx 配方：VP8/VP9 编解码（自带 configure——不是 autotools，手工
//! 跑）
//! libvpx recipe: VP8/VP9 codecs (its own configure — not autotools,
//! driven by hand)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libvpx", "linux,mac", "vpx", "--enable-libvpx", body);
    x.src = include_str!("libvpx.rs");
    x
}

/// 构建 libvpx / Build libvpx
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    sys::bash_ok(
        &format!(
            "./configure --prefix='{}' --disable-examples --disable-unit-tests \
             --disable-tools --disable-docs --disable-webm-io --disable-libyuv \
             --enable-vp9-highbitdepth --enable-static --disable-shared \
             --enable-pic",
            e.prefix
        ),
        &src,
    )?;
    sys::run_ok("make", &["-j", &e.jobs.to_string()], &src)?;
    sys::run_ok("make", &["install"], &src)
}
