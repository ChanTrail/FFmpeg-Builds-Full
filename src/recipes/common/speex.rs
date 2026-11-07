//! speex 配方：Speex 语音编解码
//! speex recipe: the Speex speech codec

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("speex", "linux,mac", "speex", "--enable-libspeex", body);
    x.src = include_str!("speex.rs");
    x
}

/// 构建 libspeex / Build libspeex
fn body(ctx: &Ctx) -> Result<(), String> {
    // downloads.xiph.org 不可达；用上游 gitlab 源码，先 autoreconf
    // downloads.xiph.org is unreachable; use the upstream gitlab source
    // and autoreconf first
    let src = ctx.src()?;
    sys::run_ok("autoreconf", &["-fiv"], &src)?;
    buildsys::autotools_build(&src, &["--disable-binaries"])
}
