//! libcurl 配方：HTTP/HTTPS 客户端（仅 macOS；旧 60-media-io.sh 移植）
//! libcurl recipe: HTTP/HTTPS client, macOS only (port of 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libcurl", "mac", "libcurl", "--enable-libcurl", body);
    x.src = include_str!("libcurl.rs");
    x
}

/// 构建 curl / Build curl
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // gnutls 探测链裸 -lgnutls 的测试程序，但我们的归档是静态的——
    // nettle/hogweed/gmp 尾巴必须由 LIBS 捎上，否则 "TLS not detected"
    // the gnutls probe links a test program against bare -lgnutls, but our
    // archive is static — its nettle/hogweed/gmp tail must ride along in
    // LIBS or configure aborts "TLS not detected"
    let libs = pkgcfg::libs_static("gnutls").unwrap_or_default();
    let _g = sys::EnvGuard::set(&[("LIBS", libs)]);
    buildsys::autotools_build(
        &src,
        &[
            "--with-gnutls",
            "--without-libpsl",
            "--without-nghttp2",
            "--without-nghttp3",
            "--without-libidn2",
            "--without-librtmp",
            "--without-libssh2",
            "--without-gssapi",
            "--disable-ldap",
            "--disable-ldaps",
            "--disable-rtsp",
            "--disable-dict",
            "--disable-telnet",
            "--disable-tftp",
            "--disable-pop3",
            "--disable-imap",
            "--disable-smb",
            "--disable-smtp",
            "--disable-gopher",
            "--disable-mqtt",
            "--disable-manual",
        ],
    )
}
