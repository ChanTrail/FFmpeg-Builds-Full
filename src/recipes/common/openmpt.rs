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
    //
    // openmpt123/examples/tests 是 AC_ARG_ENABLE 开关——旧写法
    // --without-* 设的是 with_* 变量、是死旗，三个可执行目标一直在
    // 建；openmpt123 的链接经 libopenmpt.la 的 dependency_libs 解
    // -lvorbisfile，而 vorbis 走 cmake 不产 .la——libtool 只能抓到
    // 裸 .a，传递链 -lvorbis -logg 无人带上（vorbis_synthesis_*
    // /ogg_* 全 undefined）。示例 CLI 反正不发布，整条关掉 /
    // openmpt123/examples/tests are AC_ARG_ENABLE switches — the old
    // --without-* spellings only set with_* vars, dead flags, so all
    // three executable targets kept building; openmpt123's link
    // resolves -lvorbisfile through libopenmpt.la's dependency_libs,
    // but our vorbis is cmake-built and ships no .la — libtool can
    // only reach the bare .a and the transitive -lvorbis -logg never
    // arrives (vorbis_synthesis_*/ogg_* all undefined). The example
    // CLI is never shipped anyway — turn the whole trio off
    buildsys::autotools_build(
        &src,
        &[
            "--without-portaudio",
            "--without-portaudiocpp",
            "--without-pulseaudio",
            "--without-sndfile",
            "--without-flac",
            "--without-mpg123",
            "--disable-openmpt123",
            "--disable-examples",
            "--disable-tests",
        ],
    )
}
