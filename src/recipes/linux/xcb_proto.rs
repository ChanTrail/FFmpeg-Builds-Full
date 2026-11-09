//! xcb-proto 配方：XCB 协议描述与 xcbgen 代码生成器——libxcb 前置
//! xcb-proto recipe: XCB protocol descriptions + the xcbgen code
//! generator — prerequisite for libxcb

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("xcb-proto", "linux", "xcb-proto", "-", body);
    x.src = include_str!("xcb_proto.rs");
    x
}

/// 安装协议 xml + xcbgen（libxcb configure 经 pkg-config 变量找它）
/// Install the protocol XML + xcbgen (libxcb's configure locates it via
/// a pkg-config variable)
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
