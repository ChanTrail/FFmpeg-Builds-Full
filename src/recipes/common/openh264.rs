//! openh264 配方：Cisco H.264 编解码
//! openh264 recipe: Cisco H.264 codec

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "openh264",
        "linux,mac",
        "openh264",
        "--enable-libopenh264",
        body,
    );
    x.src = include_str!("openh264.rs");
    x
}

/// 构建 openh264 / Build openh264
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(&src, &["-Dtests=disabled"])
}
