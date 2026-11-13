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
    // libxml2/bdj_jar 是 feature 项（enabled/disabled/auto，不收
    // true/false）；enable_tools 是 bool /
    // upstream ≥1.5 is meson-only (configure.ac deleted). freetype/
    // fontconfig/libxml2/bdj_jar are feature options (enabled/disabled/
    // auto — true/false rejected); enable_tools is a bool
    buildsys::meson_build(
        &src,
        &[
            "-Denable_tools=false",
            // 选项真名是 bdj_jar——autotools 时代的 --disable-bdjava-jar
            // 不带 a / the option's real name is bdj_jar — unlike the
            // autotools-era --disable-bdjava-jar it drops the "a"
            "-Dbdj_jar=disabled",
            "-Dfreetype=enabled",
            "-Dfontconfig=enabled",
            "-Dlibxml2=enabled",
        ],
    )
}
