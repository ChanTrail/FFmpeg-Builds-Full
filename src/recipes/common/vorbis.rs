//! vorbis 配方：Vorbis 音频编解码（依赖 ogg，编码侧产物是
//! vorbisenc.pc）
//! vorbis recipe: the Vorbis audio codec (depends on ogg; the encoder
//! artifact is vorbisenc.pc)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "vorbis",
        "linux,mac",
        "vorbisenc",
        "--enable-libvorbis",
        body,
    );
    x.src = include_str!("vorbis.rs");
    x
}

/// 构建 libvorbis / Build libvorbis
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &["--disable-examples"])
}
