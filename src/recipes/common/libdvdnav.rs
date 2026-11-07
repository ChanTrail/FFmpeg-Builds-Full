//! libdvdnav 配方：DVD 导航
//! libdvdnav recipe: DVD navigation

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libdvdnav",
        "linux,mac",
        "dvdnav",
        "--enable-libdvdnav",
        body,
    );
    x.src = include_str!("libdvdnav.rs");
    x
}

/// 构建 libdvdnav / Build libdvdnav
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(&src, &[])
}
