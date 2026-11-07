//! openmpt 配方：模块音乐解码（外围音频后端全关，只留核心库）
//! openmpt recipe: module-music decoding (every peripheral audio backend off — core lib only)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "openmpt",
        "linux,mac",
        "libopenmpt",
        "--enable-libopenmpt",
        body,
    );
    x.src = include_str!("openmpt.rs");
    x
}

/// 构建 libopenmpt / Build libopenmpt
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 外围后端（portaudio/pulseaudio/sndfile/flac/mpg123…）一律不拉——
    // 核心解码器自带 vorbis/ogg 支持由 vorbis 配方兜 /
    // no peripheral backends (portaudio/pulseaudio/sndfile/flac/mpg123)
    // — vorbis/ogg support in the core comes via the vorbis recipe
    buildsys::autotools_build(
        &src,
        &[
            "--without-portaudio",
            "--without-portaudiocpp",
            "--without-pulseaudio",
            "--without-sndfile",
            "--without-flac",
            "--without-mpg123",
            "--without-ltdl",
            "--without-examples",
            "--without-tests",
        ],
    )
}
