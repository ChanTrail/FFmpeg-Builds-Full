//! svtjpegxs 配方：SVT-JPEG-XS 低延迟夹层编码（旧 50-video.sh 移植）
//! svtjpegxs recipe: SVT-JPEG-XS low-latency mezzanine codec (port of
//! 50-video.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "svtjpegxs",
        "linux:x86_64,windows:x86_64",
        "SvtJpegxs,arsym:lib/libSvtJpegxs.a@svt_jpeg_xs_encoder_init",
        "--enable-libsvtjpegxs",
        body,
    );
    x.src = include_str!("svtjpegxs.rs");
    x
}

/// 构建 SVT-JPEG-XS / Build SVT-JPEG-XS
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DBUILD_APPS=OFF", "-DBUILD_TESTING=OFF"])
}
