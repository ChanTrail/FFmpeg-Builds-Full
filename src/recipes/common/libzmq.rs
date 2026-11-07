//! libzmq 配方：ZeroMQ 消息队列
//! libzmq recipe: ZeroMQ messaging

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libzmq", "linux,mac", "libzmq", "--enable-libzmq", body);
    x.src = include_str!("libzmq.rs");
    x
}

/// 构建 libzmq / Build libzmq
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_SHARED=OFF",
            "-DBUILD_STATIC=ON",
            "-DBUILD_TESTS=OFF",
            "-DZMQ_BUILD_TESTS=OFF",
            "-DWITH_DOCS=OFF",
            "-DENABLE_CPACK=OFF",
            "-DWITH_LIBSODIUM=OFF",
            "-DWITH_TLS=OFF",
        ],
    )
}
