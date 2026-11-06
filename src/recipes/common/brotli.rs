//! brotli 配方：仅 macOS（libcurl 的 --with-brotli 依赖栈补齐）
//! brotli recipe: macOS only (closes the libcurl --with-brotli dep-stack gap)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "brotli",
        "mac",
        "libbrotlicommon,libbrotlidec,libbrotlienc",
        "-",
        body,
    );
    x.src = include_str!("brotli.rs");
    x
}

/// 构建 brotli：只要库本体——cli 工具给 ffmpeg 链接行添用不上的构建时间
/// Build brotli: library only — the cli tools cost build time the link
/// can't use
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &["-DBROTLI_BUILD_TOOLS=OFF"])
}
