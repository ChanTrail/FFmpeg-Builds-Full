//! pocketsphinx 配方：语音识别
//! pocketsphinx recipe: speech recognition

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "pocketsphinx",
        "all",
        "pocketsphinx",
        "--enable-pocketsphinx",
        body,
    );
    x.src = include_str!("pocketsphinx.rs");
    x
}

/// 构建 pocketsphinx / Build pocketsphinx
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DBUILD_TESTING=OFF"])?;
    // 上游把主头文件平铺装成 include/pocketsphinx.h；FFmpeg 引用的是
    // <pocketsphinx/pocketsphinx.h>——桥接布局 / upstream installs the main
    // header flat; FFmpeg includes <pocketsphinx/pocketsphinx.h> — bridge
    let flat = e.prefix_win.join("include/pocketsphinx.h");
    let nested = e.prefix_win.join("include/pocketsphinx/pocketsphinx.h");
    if flat.is_file() && !nested.is_file() {
        fsx::install(&flat, &nested)?;
    }
    Ok(())
}
