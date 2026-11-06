//! pango 配方：librsvg 的文本布局依赖（旧 45-glib-stack.sh 移植）
//! pango recipe: librsvg's text layout dependency (port of
//! 45-glib-stack.sh)

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
            "-Dsysprof=disabled",
            "--wrap-mode=nofallback",
        ],
    )
}
