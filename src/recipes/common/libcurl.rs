//! libcurl 配方：HTTP/HTTPS 客户端（仅 macOS）
//! libcurl recipe: HTTP/HTTPS client, macOS only

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libcurl", "linux,mac", "libcurl", "--enable-libcurl", body);
    x.src = include_str!("libcurl.rs");
    x
}

/// 构建 curl / Build curl
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = crate::sys::env();
    let src = ctx.src()?;
    // autotools 路线的 config.status depfiles 在新版 make 上炸
    // （tests/* "missing separator"），走 cmake。静态归档没有链接步，
    // gnutls 的 nettle 尾巴经 .pc Requires.private 由下游传递 /
    // the autotools route's config.status depfiles blow up on newer make
    // ("missing separator" in tests/*) — take cmake. A static archive
    // has no link step; gnutls's nettle tail is forwarded to consumers
    // via .pc Requires.private
    buildsys::cmake_build(
        &src,
        &[
            "-DHTTP_ONLY=ON",
            "-DBUILD_CURL_EXE=OFF",
            "-DCURL_USE_GNUTLS=ON",
            "-DCURL_USE_LIBPSL=OFF",
            "-DCURL_USE_LIBSSH2=OFF",
            "-DCURL_BROTLI=OFF",
            "-DCURL_ZSTD=OFF",
            "-DUSE_NGHTTP2=OFF",
            "-DUSE_NGHTTP3=OFF",
            "-DUSE_NGTCP2=OFF",
            "-DUSE_QUICHE=OFF",
            "-DUSE_MSH3=OFF",
            "-DCURL_DISABLE_LDAP=ON",
            "-DCURL_DISABLE_LDAPS=ON",
            "-DBUILD_EXAMPLES=OFF",
            "-DBUILD_LIBCURL_DOCS=OFF",
            "-DBUILD_MISC_DOCS=OFF",
            "-DENABLE_CURL_MANUAL=OFF",
        ],
    )?;
    // curl 的 cmake 不产 libcurl.pc（mac 实锤不装）——pkg-config 探针
    // 全靠手写的这个；gnutls/zlib 尾巴经 Requires.private 传递 /
    // curl's cmake emits no libcurl.pc (verified on mac) — the
    // pkg-config probe relies on this hand-written one; the gnutls/
    // zlib tail forwards via Requires.private
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/libcurl.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: libcurl\nDescription: multiprotocol file transfer library\nVersion: 8.23.0\nRequires.private: gnutls zlib\nLibs: -L${{libdir}} -lcurl\nLibs.private: -lpthread\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )
}
