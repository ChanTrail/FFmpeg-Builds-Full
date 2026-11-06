//! libcdio-paranoia 配方：纠错 CD 音频抓取（旧 60-media-io.sh 移植）
//! libcdio-paranoia recipe: error-correcting CD audio ripping (port of
//! 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libcdio-paranoia",
        "linux,mac",
        "libcdio_paranoia",
        "--enable-libcdio",
        body,
    );
    x.src = include_str!("libcdio_paranoia.rs");
    x
}

/// 构建 libcdio-paranoia / Build libcdio-paranoia
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &["--disable-cxx", "--disable-example-progs"])
}
