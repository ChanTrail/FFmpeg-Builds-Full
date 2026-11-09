//! pcre2 配方：glib 的正则后端
//! pcre2 recipe: glib's regex backend

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("pcre2", "linux,mac", "libpcre2-8", "-", body);
    x.src = include_str!("pcre2.rs");
    x
}

/// 构建 pcre2-8 / Build pcre2-8
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DPCRE2_BUILD_PCRE2_8=ON",
            "-DPCRE2_BUILD_PCRE2_16=OFF",
            "-DPCRE2_BUILD_PCRE2_32=OFF",
            "-DPCRE2_BUILD_TESTS=OFF",
            "-DPCRE2_BUILD_PCRE2GREP=OFF",
            "-DPCRE2_BUILD_PCRE2TEST=OFF",
        ],
    )
}
