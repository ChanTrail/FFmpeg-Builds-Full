//! fribidi 配方：双向文本算法（pango 依赖；Windows 走 msys 包）
//! fribidi recipe: the bidirectional-text algorithm (pango dependency;
//! Windows gets it from the msys package)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("fribidi", "linux,mac", "fribidi", "-", body);
    x.src = include_str!("fribidi.rs");
    x
}

/// 构建 fribidi / Build fribidi
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(&src, &["-Ddocs=false", "-Dtests=false", "-Dbin=false"])
}
