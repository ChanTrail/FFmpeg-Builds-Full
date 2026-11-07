//! dc1394 配方：1394 摄像机控制
//! dc1394 recipe: 1394 camera control

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("dc1394", "linux", "libdc1394-2", "--enable-libdc1394", body);
    x.src = include_str!("dc1394.rs");
    x
}

/// 构建 libdc1394 / Build libdc1394
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(
        &src,
        &[
            "--disable-examples",
            "--disable-doxygen-doc",
            "--disable-doxygen-html",
            "--disable-doxygen-dot",
        ],
    )
}
