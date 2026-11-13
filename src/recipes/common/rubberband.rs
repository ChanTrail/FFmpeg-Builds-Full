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
    // mathmisc.cpp 裸用 size_t 未引 <cstddef>——clang 严格性报错，
    // 强引头文件压住（各平台同加无害）/ mathmisc.cpp uses bare size_t
    // without <cstddef> — clang strictness rejects it; force-include
    // the header (harmless on every platform)
    let _g = crate::sys::EnvGuard::append(&[("CXXFLAGS", "-include cstddef")]);
    // 只钉 fft/resampler 两个 combo 选项——其余一律默认（多出的
    // cmdline/插件二进制落进 prefix 无害，也不拉外部依赖）/
    // pin only the two combo options — everything else stays default
    // (the extra cmdline/plugin binaries land in the prefix harmlessly
    // and pull no external deps)
    // prefer_static=true：rubberband 的 dependency('sndfile') 默认非
    // static 探——只拿 Libs 公共行，而 libsndfile.a 的 mpeg 代码要
    // -lmp3lame/-lmpg123（都在 Libs.private/Requires.private 里）。
    // 不开它链接就缺 id3tag_*/lame_* 符号 / prefer_static=true:
    // rubberband's dependency('sndfile') probes non-static by default —
    // it only reads the public Libs line while libsndfile.a's mpeg
    // code needs -lmp3lame/-lmpg123 (both live in
    // Libs.private/Requires.private). Without it the link misses
    // id3tag_*/lame_* symbols
    buildsys::meson_build(
        &src,
        &[
            "-Dfft=builtin",
            "-Dresampler=builtin",
            "-Dprefer_static=true",
        ],
    )
}
