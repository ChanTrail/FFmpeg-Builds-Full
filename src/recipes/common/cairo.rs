//! cairo 配方：必须带 quartz 字体后端供 pango 使用（旧
//! 45-glib-stack.sh 移植）
//! cairo recipe: must carry the quartz font backend for pango (port of
//! 45-glib-stack.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys::{self, Os};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("cairo", "mac", "cairo,cairo-quartz", "-", body);
    x.src = include_str!("cairo.rs");
    x
}

/// 构建 cairo / Build cairo
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // cairo 1.18 删了 xml surface 和 lzma 选项（传了 meson 中止）。glib
    // 保持启用：librsvg 硬性需要 cairo-gobject。quartz：pango 在没有任何
    // cairo 字体后端时拒绝配置，Darwin 上唯一无依赖的是 cairo-quartz。
    // lzo=disabled：cairo-script 探测系统 LZO 头会拖死解释器构建。
    // cairo 1.18 dropped the xml surface and the lzma option (passing
    // either aborts meson). glib stays: librsvg hard-requires
    // cairo-gobject. quartz: pango refuses to configure with no cairo font
    // backend and cairo-quartz is the only dependency-free one on Darwin.
    // lzo off: cairo-script probes system LZO headers and dies.
    let quartz = if e.os == Os::Mac {
        "enabled"
    } else {
        "disabled"
    };
    buildsys::meson_build(
        &src,
        &[
            "-Dtests=disabled",
            "-Dspectre=disabled",
            "-Dgtk_doc=false",
            "-Dsymbol-lookup=disabled",
            "-Dlzo=disabled",
            "-Dxlib=disabled",
            "-Dxcb=disabled",
            "-Dxlib-xcb=disabled",
            "-Ddwrite=disabled",
            &format!("-Dquartz={quartz}"),
            "-Dglib=enabled",
        ],
    )
}
