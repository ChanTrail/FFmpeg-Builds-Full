//! gnutls 配方：GnuTLS（librtmp 的 rtmps 后端之一）
//! gnutls recipe: GnuTLS (one of librtmp's rtmps backends)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("gnutls", "linux,mac", "gnutls", "--enable-gnutls", body);
    x.src = include_str!("gnutls.rs");
    x
}

/// 构建 GnuTLS / Build GnuTLS
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 与 macOS 依赖脚本相同的成熟选项集 / the same proven option set as
    // the macOS dependency script
    buildsys::autotools_build(
        &src,
        &[
            "--disable-doc",
            "--disable-tools",
            "--disable-cxx",
            "--disable-tests",
            "--disable-gtk-doc-html",
            "--disable-libdane",
            "--disable-nls",
            "--enable-local-libopts",
            "--disable-guile",
            "--with-included-libtasn1",
            "--with-included-unistring",
            "--without-p11-kit",
            "--without-idn",
            "--without-libseccomp",
        ],
    )
}
