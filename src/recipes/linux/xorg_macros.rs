//! xorg-macros 配方：Xorg autoconf 宏集合——xcb/x11 链 autogen 的前置
//! xorg-macros recipe: Xorg autoconf macro set — prerequisite for the
//! xcb/x11 chain's autogen

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // 哨兵是安装落点而非库：xorg-macros.m4 是下游 autogen 要看的真身 /
    // the sentinel is an install artifact, not a library: xorg-macros.m4
    // is the real thing downstream autogen invocations look for
    let mut x = r(
        "xorg-macros",
        "linux",
        "file:share/aclocal/xorg-macros.m4",
        "-",
        body,
    );
    x.src = include_str!("xorg_macros.rs");
    x
}

/// 安装 util-macros 到前缀 share/aclocal / Install util-macros into the
/// prefix's share/aclocal
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 只产宏文件与 xorg-macros.pc（share/pkgconfig）——无库可链 /
    // installs only macro files plus xorg-macros.pc (share/pkgconfig) —
    // no library to link
    buildsys::autotools_build(&src, &[])
}
