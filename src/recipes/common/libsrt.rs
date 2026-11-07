//! libsrt 配方：SRT 安全传输协议（加密走 gnutls 栈）
//! libsrt recipe: Secure Reliable Transport (encryption via the gnutls stack)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libsrt", "linux,mac", "srt", "--enable-libsrt", body);
    x.src = include_str!("libsrt.rs");
    x
}

/// 构建 libsrt / Build libsrt
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // USE_ENCLIB=gnutls：加密后端用前缀自造的 gnutls（openssl 在
    // linux 上是系统动态库，链接即踩 ldd 白名单）/ USE_ENCLIB=gnutls:
    // the crypto backend rides our prefix gnutls (system openssl on linux
    // is a dynamic lib — linking it trips the ldd allowlist)
    buildsys::cmake_build(
        &src,
        &[
            "-DENABLE_SHARED=OFF",
            "-DENABLE_STATIC=ON",
            "-DENABLE_APPS=OFF",
            "-DENABLE_TESTING=OFF",
            "-DUSE_ENCLIB=gnutls",
        ],
    )
}
