//! x264 配方：H.264 编码器（自带 configure——不是 autotools，旗标集合
//! 不同，手工跑）
//! x264 recipe: the H.264 encoder (its own configure — not autotools,
//! different flag set, driven by hand)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("x264", "linux,mac", "x264", "--enable-libx264", body);
    x.src = include_str!("x264.rs");
    x
}

/// 构建 x264 / Build x264
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 禁用一切可选输入（avs/swscale/lavf/ffms/gpac/lsmash）——只要编码
    // 库本体 / every optional input off (avs/swscale/lavf/ffms/gpac/
    // lsmash) — encoder library only
    sys::bash_ok(
        &format!(
            "./configure --prefix='{}' --enable-static --disable-shared \
             --enable-pic --disable-cli --disable-opencl --disable-avs \
             --disable-swscale --disable-lavf --disable-ffms --disable-gpac \
             --disable-lsmash",
            e.prefix
        ),
        &src,
    )?;
    sys::run_ok("make", &["-j", &e.jobs.to_string()], &src)?;
    sys::run_ok("make", &["install"], &src)
}
