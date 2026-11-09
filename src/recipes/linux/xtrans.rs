//! xtrans 配方：X 传输层抽象头（libX11 configure 的 PKG_CHECK 要求）
//! xtrans recipe: X transport abstraction headers (required by
//! libX11's configure PKG_CHECK)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // 纯头文件包：xtrans.pc 无 Libs——用文件验收 / header-only package:
    // xtrans.pc carries no Libs — file acceptance instead
    let mut x = r(
        "xtrans",
        "linux",
        "file:include/X11/Xtrans/Xtrans.h",
        "-",
        body,
    );
    x.src = include_str!("xtrans.rs");
    x
}

/// 安装 xtrans 头文件 / Install the xtrans headers
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
