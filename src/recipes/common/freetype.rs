//! freetype 配方：字体渲染引擎（librsvg 的 Rust 侧经 pkg-config 找
//! libfreetype.a——前缀没有时 meson 的 CMake 兜底会命中 brew 的 dylib，
//! cargo 静态链接即暴毙；pango 的 pangoft2 后端也用它）
//! freetype recipe: the font rasterizer (librsvg's Rust side wants
//! libfreetype.a via pkg-config — absent from the prefix, meson's CMake
//! fallback lands on brew's dylib-only freetype and the static link
//! explodes; pango's pangoft2 backend uses it too)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("freetype", "mac", "freetype2", "-", body);
    x.src = include_str!("freetype.rs");
    x
}

/// 构建 freetype / Build freetype
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 可选依赖全关：auto 会被 brew/prefix 里碰巧存在的库点亮，把
    // zlib/brotli/png 写进 freetype2.pc 的 Requires；harfbuzz 必须关——
    // freetype↔harfbuzz 互启是循环依赖 / every optional dep off: auto
    // lights up whatever sits in brew/the prefix and leaks zlib/brotli/
    // png into freetype2.pc's Requires; harfbuzz must stay off —
    // freetype<->harfbuzz mutual enablement is circular
    buildsys::meson_build(
        &src,
        &[
            "-Dzlib=disabled",
            "-Dbzip2=disabled",
            "-Dbrotli=disabled",
            "-Dpng=disabled",
            "-Dharfbuzz=disabled",
            "-Dtests=disabled",
            "--wrap-mode=nofallback",
        ],
    )
}
