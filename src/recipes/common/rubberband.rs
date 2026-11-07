//! rubberband 配方：音高/变速库（fft/resampler 全用内置实现，零外部依赖）
//! rubberband recipe: pitch/time-stretching (builtin fft/resampler — zero external deps)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "rubberband",
        "linux,mac",
        "rubberband",
        "--enable-librubberband",
        body,
    );
    x.src = include_str!("rubberband.rs");
    x
}

/// 构建 librubberband / Build librubberband
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 只钉 fft/resampler 两个 combo 选项——其余一律默认（多出的
    // cmdline/插件二进制落进 prefix 无害，也不拉外部依赖）/
    // pin only the two combo options — everything else stays default
    // (the extra cmdline/plugin binaries land in the prefix harmlessly
    // and pull no external deps)
    buildsys::meson_build(&src, &["-Dfft=builtin", "-Dresampler=builtin"])
}
