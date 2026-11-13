//! libssh 配方：SSH/SFTP（crypto 走 mbedtls——openssl 是动态库，链了过不了 ldd/框架白名单）
//! libssh recipe: SSH/SFTP (crypto via mbedtls — openssl is a dynamic lib and would trip the ldd/framework allowlist)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libssh", "linux,mac", "libssh", "--enable-libssh", body);
    x.src = include_str!("libssh.rs");
    x
}

/// 构建 libssh / Build libssh
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_SHARED_LIBS=OFF",
            "-DWITH_EXAMPLES=OFF",
            "-DWITH_SERVER=OFF",
            "-DWITH_TESTING=OFF",
            "-DUNIT_TESTING=OFF",
            "-DWITH_GSSAPI=OFF",
            "-DWITH_ZLIB=ON",
            "-DWITH_MBEDTLS=ON",
        ],
    )?;
    // 上游 libssh.pc 不带 crypto 后端归档——pki_mbedcrypto/libmbedcrypto
    // 的 mbedtls_* 引用静态链下游拿不到（mac 上 FFmpeg configure
    // check_func_headers 链测全灭）。手补三段归档；mbedcrypto 的
    // -lpthread 尾巴由 mbedtls 配方自己喂过 / upstream libssh.pc
    // carries no crypto-backend archives — the mbedtls_* references in
    // pki_mbedcrypto/libmbedcrypto never reach static consumers (they
    // kill FFmpeg's check_func_headers link test on mac). Add the three
    // archives by hand; mbedcrypto's -lpthread tail is already fed by
    // the mbedtls recipe
    crate::pkgcfg::libs_private("libssh", &["-lmbedtls", "-lmbedx509", "-lmbedcrypto"]);
    Ok(())
}
