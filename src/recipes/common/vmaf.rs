//! vmaf 配方：视频质量度量（meson 根在 libvmaf 子目录，模型内嵌进库）
//! vmaf recipe: video quality metrics (meson root in libvmaf/, models baked in)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("vmaf", "linux,mac", "libvmaf", "--enable-libvmaf", body);
    x.src = include_str!("vmaf.rs");
    x
}

/// 构建 libvmaf / Build libvmaf
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // meson 工程在 libvmaf/ 子目录；built_in_models 把模型打进库——
    // 独立 ffmpeg 不带 vmaf/model 数据目录 / the meson project lives in
    // libvmaf/; built_in_models embeds the models — a standalone ffmpeg
    // carries no vmaf/model data dir
    buildsys::meson_build(
        &src.join("libvmaf"),
        &[
            "-Denable_tests=false",
            "-Denable_docs=false",
            "-Dbuilt_in_models=true",
        ],
    )
}
