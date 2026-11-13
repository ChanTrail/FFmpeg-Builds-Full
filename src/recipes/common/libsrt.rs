//! libsrt 配方：SRT 安全传输协议（加密走 mbedtls 栈）
//! libsrt recipe: Secure Reliable Transport (encryption via the mbedtls stack)

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
    // USE_ENCLIB=mbedtls：加密后端用前缀自造的 mbedtls。gnutls 路径的
    // cryspr-gnutls 还在调 nettle 老 API（aes_set_encrypt_key），新版
    // nettle 已删除→隐式声明编译挂；openssl 是系统动态库会踩 ldd
    // 白名单 / USE_ENCLIB=mbedtls rides our prefix mbedtls — the gnutls
    // path's cryspr-gnutls still calls the old nettle API
    // (aes_set_encrypt_key) which newer nettle deleted → implicit
    // decl; openssl is a system dylib that trips the ldd allowlist
    buildsys::cmake_build(
        &src,
        &[
            "-DENABLE_SHARED=OFF",
            "-DENABLE_STATIC=ON",
            "-DENABLE_APPS=OFF",
            "-DENABLE_TESTING=OFF",
            "-DUSE_ENCLIB=mbedtls",
        ],
    )
}
