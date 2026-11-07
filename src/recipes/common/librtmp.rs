//! librtmp 配方：RTMP 流媒体（rtmpdump）
//! librtmp recipe: RTMP streaming, rtmpdump

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("librtmp", "linux,mac", "librtmp", "--enable-librtmp", body);
    x.src = include_str!("librtmp.rs");
    x
}

/// 构建 librtmp / Build librtmp
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // git:// 原生协议——https 端点周期性 502，9418 端口直接提供同一仓库
    // git:// native protocol — the https endpoint 502s periodically; port
    // 9418 serves the same repo
    let src = ctx.src()?;
    // 优先 GnuTLS（只需要 nettle+hogweed）。rtmpdump 的 OpenSSL 代码早于
    // OpenSSL 3 不透明结构体、已编不过；都没有时库照样建（无 rtmps）
    // GnuTLS first (needs only nettle+hogweed). The OpenSSL code predates
    // OpenSSL 3's opaque structs; with neither backend the library still
    // builds — just without rtmps
    let crypto = if pkgcfg::exists("gnutls") {
        "GNUTLS"
    } else if pkgcfg::exists("openssl < 3.0.0") {
        "OPENSSL"
    } else {
        ""
    };
    if crypto == "GNUTLS" {
        // USE_GNUTLS 路径调 nettle 的 *_digest(ctx,LEN,out)——nettle 4.0
        // 删了长度参数。改写所有调用点 / the USE_GNUTLS path calls
        // nettle's *_digest(ctx,LEN,out) — nettle 4.0 removed the length
        // parameter. Rewrite every call site.
        for f in ["librtmp/rtmp.c", "librtmp/handshake.h", "librtmp/hashswf.c"] {
            // ERE：裸 (...) 分组 / | 选择，\( 为字面左括号
            // ERE: bare (...) groups / | alternation, \( is literal lparen
            fsx::sed_i_ere(
                "s/(hmac_)?(md5|sha1|sha256)_digest\\(([^,()]+),[[:space:]]*[A-Z0-9_]*_DIGEST_LENGTH,[[:space:]]*/\\1\\2_digest(\\3, /g",
                &src.join(f),
                &src,
            )?;
        }
    }
    // install_base 装静态库、公共头和正确生成的 librtmp.pc；CC 必须放
    // 命令行，Makefile 无条件覆盖它 / install_base installs the static
    // lib, public headers and a proper librtmp.pc; CC must go on the
    // command line since the Makefile assigns it unconditionally
    let cflags = std::env::var("CFLAGS").unwrap_or_default();
    let ldflags = std::env::var("LDFLAGS").unwrap_or_default();
    sys::run_ok(
        "make",
        &[
            "-C",
            "librtmp",
            "librtmp.a",
            "librtmp.pc",
            "SYS=posix",
            &format!("CRYPTO={crypto}"),
            &format!("prefix={}", e.prefix),
            &format!("CC={}", e.cc),
            &format!("XCFLAGS={cflags}"),
            &format!("XLDFLAGS={ldflags}"),
        ],
        &src,
    )?;
    sys::run_ok(
        "make",
        &[
            "-C",
            "librtmp",
            "install_base",
            "SYS=posix",
            &format!("prefix={}", e.prefix),
        ],
        &src,
    )
}
