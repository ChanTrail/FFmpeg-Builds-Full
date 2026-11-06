//! aribb24 配方：ARIB STD-B24 字幕（FFmpeg 要的版本比最后一个 tag 新；
//! 旧 60-media-io.sh 移植）
//! aribb24 recipe: ARIB STD-B24 captions; FFmpeg wants a version newer
//! than the last tag (port of 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("aribb24", "mac", "aribb24", "--enable-libaribb24", body);
    x.src = include_str!("aribb24.rs");
    x
}

/// 构建 aribb24 / Build aribb24
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    fsx::sed_i("s/1\\.0\\.3/1.0.4/", &src.join("configure.ac"), &src)?;
    sys::run_ok("autoreconf", &["-fiv"], &src)?;
    buildsys::autotools_build(&src, &[])
}
