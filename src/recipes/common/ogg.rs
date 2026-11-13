//! ogg 配方：Ogg 容器（vorbis/theora 的前置）
//! ogg recipe: the Ogg container (vorbis/theora prerequisite)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("ogg", "linux,mac", "ogg", "-", body);
    x.src = include_str!("ogg.rs");
    x
}

/// 构建 libogg / Build libogg
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
