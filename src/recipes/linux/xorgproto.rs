//! xorgproto 配方：X 协议头文件全集（xproto/xextproto/kbproto 等）
//! xorgproto recipe: the X protocol header set (xproto/xextproto/
//! kbproto etc.)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("xorgproto", "linux", "xproto", "-", body);
    x.src = include_str!("xorgproto.rs");
    x
}

/// 安装 X 协议头文件 / Install the X protocol headers
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // --disable-specs：文档要 xmlto/xorg-sgml-doctools，纯头文件用不上 /
    // --disable-specs: the docs want xmlto/xorg-sgml-doctools — pure
    // headers don't need them
    buildsys::autotools_build(&src, &["--disable-specs"])
}
