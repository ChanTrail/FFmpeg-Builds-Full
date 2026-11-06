//! gmp 配方：多精度运算库（nettle/gnutls 依赖）
//! gmp recipe: the arbitrary-precision library (nettle/gnutls dependency)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("gmp", "all", "file:lib/libgmp.a", "-", body);
    x.src = include_str!("gmp.rs");
    x
}

/// 构建 GMP / Build GMP
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
