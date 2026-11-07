//! harfbuzz 配方：OpenType 整形引擎（pango.pc 的公共 Requires 依赖；Windows
//! 走 msys 包）
//! harfbuzz recipe: the OpenType shaping engine (public Requires of pango.pc;
//! Windows gets it from the msys package)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("harfbuzz", "mac", "harfbuzz", "-", body);
    x.src = include_str!("harfbuzz.rs");
    x
}

/// 构建 harfbuzz / Build harfbuzz
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 可选后端除两处例外全关：开着的 auto 特性会被 brew/prefix 里碰巧
    // 存在的库点亮，再把它们写进子 pc 的 Requires。--wrap-mode=nofallback
    // 同理防 wrap 子工程静默覆盖 / every optional backend off except two:
    // each enabled auto feature gets lit by whatever happens to sit in
    // brew/the prefix and then leaks into a sub-pc's Requires.
    // --wrap-mode=nofallback likewise blocks a wrap subproject from
    // silently shadowing the install.
    buildsys::meson_build(
        &src,
        &[
            // coretext 是要开着的后端之一：pango 的 pangocoretext.c 引用
            // hb-coretext.h——该头只在 HAVE_CORETEXT 时装进 include/harfbuzz，
            // 且上游此选项默认 disabled（不会 auto）。只链 Apple 系统框架，
            // 不引入外部依赖 / coretext is one backend that must stay on:
            // pango's pangocoretext.c includes hb-coretext.h — a header
            // only installed under HAVE_CORETEXT, and upstream defaults
            // this option to disabled (no auto). It only links Apple system
            // frameworks, pulling in no external deps
            "-Dcoretext=enabled",
            "-Dglib=disabled",
            "-Dgobject=disabled",
            "-Dcairo=disabled",
            // freetype 是第二个例外：pango -Dfontconfig=enabled 后编
            // pangoft2，pangofc/pango-ot 引用 hb-ft.h + hb_ft_*——上游把
            // hb-ft 子集并进主 libharfbuzz（HAVE_FREETYPE 下 hb_ft_sources
            // 折进 hb_sources），freetype2 自动落进 harfbuzz.pc 的
            // Requires.private / freetype is the second exception: with
            // pango -Dfontconfig=enabled, pangoft2 compiles pangofc/
            // pango-ot against hb-ft.h + hb_ft_* — upstream folds the
            // hb-ft subset into the main libharfbuzz (hb_ft_sources merge
            // into hb_sources under HAVE_FREETYPE) and freetype2 lands in
            // harfbuzz.pc's Requires.private on its own
            "-Dfreetype=enabled",
            "-Dicu=disabled",
            "-Dgraphite2=disabled",
            "-Dzlib=disabled",
            "-Dpng=disabled",
            "-Dchafa=disabled",
            "-Dtests=disabled",
            "-Dintrospection=disabled",
            "-Ddocs=disabled",
            "-Dutilities=disabled",
            "-Dbenchmark=disabled",
            "-Dsubset=disabled",
            "--wrap-mode=nofallback",
        ],
    )
}
