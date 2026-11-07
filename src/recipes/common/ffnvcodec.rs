//! ffnvcodec 配方：NVIDIA nv-codec-headers——纯头文件 + ffnvcodec.pc，
//! make install 一步到位
//! ffnvcodec recipe: NVIDIA nv-codec-headers — headers plus ffnvcodec.pc
//! via a single make install

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "ffnvcodec",
        "linux,windows",
        "ffnvcodec",
        "--enable-ffnvcodec",
        body,
    );
    x.src = include_str!("ffnvcodec.rs");
    x
}

/// 安装 nv-codec-headers / Install nv-codec-headers
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // 源键是 nv-codec-headers 而非配方名 / the sources key is
    // nv-codec-headers, not the recipe name
    let src = ctx.src_named("nv-codec-headers")?;
    sys::run_ok("make", &["install", &format!("PREFIX={}", e.prefix)], &src)
}
