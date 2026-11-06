//! raw1394 配方：IEEE1394 采集（旧 70-linux-hw.sh 移植）
//! raw1394 recipe: IEEE1394 capture (port of 70-linux-hw.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("raw1394", "linux", "libraw1394", "-", body);
    x.src = include_str!("raw1394.rs");
    x
}

/// 构建 libraw1394 / Build libraw1394
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
