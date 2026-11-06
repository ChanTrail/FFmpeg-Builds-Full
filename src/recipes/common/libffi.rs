//! libffi 配方：glib/gobject 依赖（旧 45-glib-stack.sh 移植）
//! libffi recipe: glib/gobject dependency (port of 45-glib-stack.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libffi", "mac", "libffi", "-", body);
    x.src = include_str!("libffi.rs");
    x
}

/// 构建 libffi / Build libffi
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &["--disable-docs"])
}
