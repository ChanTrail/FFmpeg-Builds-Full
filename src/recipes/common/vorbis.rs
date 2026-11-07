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
    // autotools 路线的 libtool 在 darwin 链接 test 程序时硬塞
    // -force_cpusubtype_ALL（ppc 时代死旗，现代 ld 已删）——cmake 没这
    // 毛病，且同样装 vorbis*.pc / the autotools route's libtool jams
    // -force_cpusubtype_ALL into darwin test-program links (a ppc-era
    // flag long removed from modern ld) — cmake carries no such quirk
    // and still installs vorbis*.pc
    buildsys::cmake_build(&src, &["-DBUILD_TESTING=OFF"])
}
