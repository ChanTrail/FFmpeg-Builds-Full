//! libjxl 配方：JPEG XL（hwy/skcms/brotli 以 submodule 随源树携带，全部
//! 内置）
//! libjxl recipe: JPEG XL (hwy/skcms/brotli ride as submodules — fully
//! bundled)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libjxl", "linux,mac", "libjxl", "--enable-libjxl", body);
    x.src = include_str!("libjxl.rs");
    x
}

/// 构建 libjxl / Build libjxl
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_TESTING=OFF",
            "-DJPEGXL_ENABLE_TOOLS=OFF",
            "-DJPEGXL_ENABLE_JPEGLI=OFF",
            "-DJPEGXL_ENABLE_JPEGLI_LIBJPEG=OFF",
            "-DJPEGXL_ENABLE_DOXYGEN=OFF",
            "-DJPEGXL_ENABLE_MANPAGES=OFF",
            "-DJPEGXL_ENABLE_BENCHMARK=OFF",
            "-DJPEGXL_ENABLE_EXAMPLES=OFF",
            "-DJPEGXL_ENABLE_OPENEXR=OFF",
            "-DJPEGXL_ENABLE_SKCMS=ON",
            "-DJPEGXL_ENABLE_VIEWERS=OFF",
            "-DJPEGXL_ENABLE_JNI=OFF",
            "-DJPEGXL_ENABLE_PLUGINS=OFF",
            "-DJPEGXL_ENABLE_DEVTOOLS=OFF",
            "-DJPEGXL_ENABLE_SJPEG=OFF",
        ],
    )
}
