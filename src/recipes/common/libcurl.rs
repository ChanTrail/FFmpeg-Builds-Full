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
            // picky 探针在 GCC16 上采到 -Wleading-whitespace/-Wtrailing-
            // whitespace——gnutls 头（几千行 openpgp.h 等）每行一报，
            // 每个 TU 喷几万条警告把 extras 的 325min 超时活生生拖死。
            // 整组 picky 关掉：产物不依赖警告集 / the picky probe picks
            // up -Wleading-whitespace/-Wtrailing-whitespace on GCC16 —
            // gnutls headers (thousands of lines like openpgp.h) warn on
            // nearly every line, tens of thousands of warnings per TU,
            // and the spam drags the build past extras' 325min timeout.
            // The whole picky set goes off: the artifact doesn't depend
            // on the warning flags
            "-DPICKY_COMPILER=OFF",
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
            // libidn2 默认开着会去摸系统/brew 装的副本：检出后 idn2_*
            // 引用烙进 libcurl.a，而 -lidn2 进不了我们的手写 .pc——静态
            // 链必炸；FFmpeg 的 URL 用不到 IDNA / USE_LIBIDN2 defaults
            // ON and sniffs out a system/brew copy: detected builds bake
            // idn2_* references into libcurl.a while -lidn2 can't reach
            // our hand-written .pc — static links die. FFmpeg's URLs
            // don't need IDNA
            "-DUSE_LIBIDN2=OFF",
        ],
    )?;
    // curl 的 cmake 不产 libcurl.pc（mac 实锤不装）——pkg-config 探针
    // 全靠手写的这个；gnutls/zlib 尾巴经 Requires.private 传递 /
    // curl's cmake emits no libcurl.pc (verified on mac) — the
    // pkg-config probe relies on this hand-written one; the gnutls/
    // zlib tail forwards via Requires.private
    //
    // macOS 代理探测（macos.c → SCDynamicStoreCopyProxies）无条件编译，
    // 链时必须 -framework SystemConfiguration；linux 无此框架 / macOS
    // proxy detection (macos.c -> SCDynamicStoreCopyProxies) compiles
    // unconditionally and needs -framework SystemConfiguration at link
    // time; linux has no such framework
    let libs_private = if e.os == crate::sys::Os::Mac {
        "-lpthread -framework SystemConfiguration"
    } else {
        "-lpthread"
    };
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/libcurl.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: libcurl\nDescription: multiprotocol file transfer library\nVersion: 8.23.0\nRequires.private: gnutls zlib\nLibs: -L${{libdir}} -lcurl\nLibs.private: {}\nCflags: -I${{includedir}}\n",
            e.prefix, libs_private
        ),
    )
}
