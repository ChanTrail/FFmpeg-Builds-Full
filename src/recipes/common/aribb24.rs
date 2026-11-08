//! aribb24 配方：ARIB STD-B24 字幕（FFmpeg 要的版本比最后一个 tag 新）
//! aribb24 recipe: ARIB STD-B24 captions; FFmpeg wants a version newer
//! than the last tag

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // linux 也走源码：Debian 的 libaribb24-dev 只有 1.0.3，过不了 FFmpeg
    // 的 >1.0.3 pkg-config 检查（就是下面 sed 抬版本的原因）/
    // linux builds from source too: Debian's libaribb24-dev is only 1.0.3
    // and fails FFmpeg's >1.0.3 pkg-config check (why the sed bump below
    // exists)
    let mut x = r(
        "aribb24",
        "linux,mac",
        "aribb24",
        "--enable-libaribb24",
        body,
    );
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
