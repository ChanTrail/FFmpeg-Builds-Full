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
    // bdjava 要 JDK——无头 CLI 用不上，关掉 /
    // BD-J wants a JDK — useless for a headless CLI, turn it off
    buildsys::autotools_build(&src, &["--disable-bdjava-jar", "--disable-doxygen-doc"])
}
