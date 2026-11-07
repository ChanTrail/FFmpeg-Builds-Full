//! libbluray 配方：蓝光导航（BD-J jar/doxygen 全关；freetype/fontconfig/libxml2 自动探测）
//! libbluray recipe: Blu-ray navigation (BD-J jar/doxygen off; freetype/fontconfig/libxml2 auto-detected)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libbluray",
        "linux,mac",
        "libbluray",
        "--enable-libbluray",
        body,
    );
    x.src = include_str!("libbluray.rs");
    x
}

/// 构建 libbluray / Build libbluray
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 上游 ≥1.5 已纯 meson（configure.ac 删了）。freetype/fontconfig/
    // libxml2 是 feature 项显式开；enable_tools/bdjava_jar 是 bool /
    // upstream ≥1.5 is meson-only (configure.ac deleted). freetype/
    // fontconfig/libxml2 are feature options flipped on explicitly;
    // enable_tools/bdjava_jar are bools
    buildsys::meson_build(
        &src,
        &[
            "-Denable_tools=false",
            // 选项真名是 bdj_jar——autotools 时代的 --disable-bdjava-jar
            // 不带 a / the option's real name is bdj_jar — unlike the
            // autotools-era --disable-bdjava-jar it drops the "a"
            "-Dbdj_jar=false",
            "-Dfreetype=enabled",
            "-Dfontconfig=enabled",
            "-Dlibxml2=enabled",
        ],
    )
}
