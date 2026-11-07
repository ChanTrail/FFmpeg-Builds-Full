//! mbedtls 配方：轻量 TLS/crypto 栈（libssh 的加密后端——openssl 在 linux/mac 上都是动态库不能链）
//! mbedtls recipe: the lightweight TLS/crypto stack (libssh's crypto backend — openssl is dynamic on linux/mac and can't be linked)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("mbedtls", "linux,mac", "file:lib/libmbedtls.a", "-", body);
    x.src = include_str!("mbedtls.rs");
    x
}

/// 构建 mbedtls / Build mbedtls
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DENABLE_TESTING=OFF",
            "-DENABLE_PROGRAMS=OFF",
            "-DUSE_SHARED_MBEDTLS_LIBRARY=OFF",
            "-DUSE_STATIC_MBEDTLS_LIBRARY=ON",
            "-DMBEDTLS_FATAL_WARNINGS=OFF",
        ],
    )
}
