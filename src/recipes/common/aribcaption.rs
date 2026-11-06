//! aribcaption 配方：ARIB 字幕渲染（旧 60-media-io.sh 移植）
//! aribcaption recipe: ARIB caption rendering (port of 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "aribcaption",
        "mac",
        "libaribcaption",
        "--enable-libaribcaption",
        body,
    );
    x.src = include_str!("aribcaption.rs");
    x
}

/// 构建 libaribcaption / Build libaribcaption
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DARIBCC_BUILD_TESTS=OFF",
            "-DARIBCC_SHARED_LIBRARY=OFF",
            "-DARIBCC_NO_EXCEPTIONS=OFF",
        ],
    )
}
