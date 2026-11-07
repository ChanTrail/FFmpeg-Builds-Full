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
    // 可选后端全关：pango 只要核心 harfbuzz；开着的 auto 特性会被
    // brew/prefix 里碰巧存在的库点亮，再把它们写进子 pc 的 Requires。
    // --wrap-mode=nofallback 同理防 wrap 子工程静默覆盖 / every optional
    // backend off: pango only needs the core lib; each enabled auto feature
    // gets lit by whatever happens to sit in brew/the prefix and then leaks
    // into a sub-pc's Requires. --wrap-mode=nofallback likewise blocks a
    // wrap subproject from silently shadowing the install.
    buildsys::meson_build(
        &src,
        &[
            // coretext 是唯一要开着的后端：pango 的 pangocoretext.c 引用
            // hb-coretext.h——该头只在 HAVE_CORETEXT 时装进 include/harfbuzz，
            // 且上游此选项默认 disabled（不会 auto）。只链 Apple 系统框架，
            // 不引入外部依赖 / coretext is the one backend that must stay
            // on: pango's pangocoretext.c includes hb-coretext.h — a header
            // only installed under HAVE_CORETEXT, and upstream defaults
            // this option to disabled (no auto). It only links Apple system
            // frameworks, pulling in no external deps
            "-Dcoretext=enabled",
            "-Dglib=disabled",
            "-Dgobject=disabled",
            "-Dcairo=disabled",
            "-Dfreetype=disabled",
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
