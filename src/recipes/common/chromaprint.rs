//! chromaprint 配方：声学指纹（kissfft 以 submodule 随源码走，零外部 FFT 依赖）
//! chromaprint recipe: acoustic fingerprinting (kissfft rides as a submodule — no external FFT dep)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "chromaprint",
        "linux,mac",
        "libchromaprint",
        "--enable-chromaprint",
        body,
    );
    x.src = include_str!("chromaprint.rs");
    x
}

/// 构建 chromaprint / Build chromaprint
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_TOOLS=OFF",
            "-DBUILD_TESTS=OFF",
            "-DFFT_LIB=kissfft",
        ],
    )
}
