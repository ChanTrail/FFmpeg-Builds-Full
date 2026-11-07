//! qrencode 配方：二维码生成
//! qrencode recipe: QR code generation

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "qrencode",
        "linux,mac,windows",
        "libqrencode",
        "--enable-libqrencode",
        body,
    );
    x.src = include_str!("qrencode.rs");
    x
}

/// 构建 libqrencode / Build libqrencode
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &["-DWITH_TOOLS=NO", "-DWITH_TESTS=NO", "-DWITHOUT_PNG=ON"],
    )
}
