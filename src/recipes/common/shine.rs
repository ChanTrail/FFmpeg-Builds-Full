//! shine 配方：定点 MP3 编码器（旧 30-audio.sh 移植）
//! shine recipe: fixed-point MP3 encoder (port of 30-audio.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("shine", "linux,mac", "shine", "--enable-libshine", body);
    x.src = include_str!("shine.rs");
    x
}

/// 构建 libshine / Build libshine
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    sys::run_ok("autoreconf", &["-fiv"], &src)?;
    // K&R 时代的声明与 C23 的 `()` 冲突；按 gnu89 编译
    // K&R-era declarations collide with C23's `()`; build as gnu89
    let _g = sys::EnvGuard::append(&[("CFLAGS", "-std=gnu89")]);
    buildsys::autotools_build(&src, &[])
}
