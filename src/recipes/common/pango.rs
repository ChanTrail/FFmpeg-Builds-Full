//! pango 配方：librsvg 的文本布局依赖
//! pango recipe: librsvg's text layout dependency

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("pango", "mac", "pangocairo", "-", body);
    x.src = include_str!("pango.rs");
    x
}

/// 构建 pango / Build pango
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // --wrap-mode=nofallback：harfbuzz.pc 解析失败时 meson 默认把它当 wrap
    // 子工程构建并装到 workspace 归档之上——静默污染 vendored 构建。让它
    // 响亮地失败 / when harfbuzz.pc fails to parse, meson's default builds
    // it as a wrap subproject AND installs it over the workspace archive —
    // silently corrupting the vendored build. Fail loudly instead.
    buildsys::meson_build(
        &src,
        &[
            "-Dintrospection=disabled",
            "-Dbuild-testsuite=false",
            "-Dbuild-examples=false",
            "-Dxft=disabled",
            "-Dlibthai=disabled",
            // fontconfig=enabled：librsvg 的 rsvg-convert 在 unix 上无条件
            // extern FcGetVersion，需要 pangoft2 链出来的 -lfontconfig；
            // auto 探测会在 fontconfig 缺席时静默产无 pangoft2 的 pango，
            // 死法挪到 rsvg-convert 链接才爆。显式 enabled 响亮地约束
            // fontconfig 配方先就位 / fontconfig=enabled: librsvg's
            // rsvg-convert unconditionally externs FcGetVersion on unix
            // and needs -lfontconfig via pangoft2; auto-detect would
            // silently produce a pangoft2-less pango when fontconfig is
            // absent and defer the death to rsvg-convert's link. Explicit
            // enabled loudly requires the fontconfig recipe to land first
            "-Dfontconfig=enabled",
            "-Dsysprof=disabled",
            "--wrap-mode=nofallback",
        ],
    )
}
