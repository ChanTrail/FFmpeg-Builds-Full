//! libx11 配方：X11 客户端库（xlib 特性的本体）
//! libx11 recipe: the X11 client library (the body of the xlib feature)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libx11", "linux", "x11", "-", body);
    x.src = include_str!("libx11.rs");
    x
}

/// 构建 libX11 静态库 / Build libX11 statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // --disable-specs：文档要 xmlto/xorg-sgml-doctools；--without-locale
    // 之类一律不动（运行时 locale 表是数据文件照常装）/ --disable-specs:
    // docs want xmlto/xorg-sgml-doctools; locale data files still install
    // as usual
    buildsys::autotools_build(&src, &["--disable-specs"])
}
