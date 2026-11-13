//! frei0r 配方：极简视频插件 API——FFmpeg 只要 frei0r.h，插件运行时
//! dlopen，不编进来
//! frei0r recipe: the minimal video-plugin API — FFmpeg wants only
//! frei0r.h; plugins are dlopen'd at runtime, never linked in

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "frei0r",
        "linux,mac",
        "file:include/frei0r.h",
        "--enable-frei0r",
        body,
    );
    x.src = include_str!("frei0r.rs");
    x
}

/// 安装 frei0r.h / Install frei0r.h
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let dst = e.prefix_win.join("include");
    fsx::install_d(&dst)?;
    fsx::install(&src.join("include/frei0r.h"), &dst.join("frei0r.h"))
}
