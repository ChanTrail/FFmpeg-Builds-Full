//! openjpeg 配方：JPEG2000 编解码
//! openjpeg recipe: the JPEG2000 codec

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "openjpeg",
        "linux,mac",
        "libopenjp2",
        "--enable-libopenjpeg",
        body,
    );
    x.src = include_str!("openjpeg.rs");
    x
}

/// 构建 openjpeg / Build openjpeg
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_CODEC=OFF",
            "-DBUILD_TESTING=OFF",
            "-DBUILD_THIRDPARTY=OFF",
            "-DBUILD_DOC=OFF",
            "-DBUILD_PKGCONFIG_FILES=ON",
        ],
    )
}
