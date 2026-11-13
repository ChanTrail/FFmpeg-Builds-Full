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
        // 纯头文件——ffnvcodec.pc 没有 Libs/-l 记号，pc 检查必败，
        // 改查头文件（file: 按 $PREFIX 字面相对，include/ 层要写全）/
        // header-only — ffnvcodec.pc carries no Libs/-l token so a pc
        // check always fails; check the header instead (file: resolves
        // literally under $PREFIX — the include/ level is explicit)
        "file:include/ffnvcodec/nvEncodeAPI.h",
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
