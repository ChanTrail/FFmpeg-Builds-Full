//! libxdmcp 配方：XDM 控制协议库（libxcb 的 XdmcpWrap 依赖）
//! libxdmcp recipe: XDM control protocol library (libxcb wants
//! XdmcpWrap)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libxdmcp", "linux", "xdmcp", "-", body);
    x.src = include_str!("libxdmcp.rs");
    x
}

/// 构建 libXdmcp 静态库 / Build libXdmcp statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(&src, &[])
}
