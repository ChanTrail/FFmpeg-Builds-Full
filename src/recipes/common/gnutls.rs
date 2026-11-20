//! gnutls 配方：GnuTLS（librtmp 的 rtmps 后端之一）
//! gnutls recipe: GnuTLS (one of librtmp's rtmps backends)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("gnutls", "linux,mac", "gnutls", "--enable-gnutls", body);
    x.src = include_str!("gnutls.rs");
    x
}

/// 构建 GnuTLS / Build GnuTLS
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
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
            // 系统优先级文件 /etc/gnutls/config（发行级加密策略，如禁
            // 旧协议）默认取 $sysconfdir/gnutls/config → 烤
            // /opt/ffbuild/etc/gnutls/config，用户机上的策略文件永远
            // 读不到（缺席容忍，但静默丢策略）。按 OS 指系统标准位；
            // 证书店 configure 期已探测绝对路径无需动 /
            // the system priority file /etc/gnutls/config (distro-wide
            // crypto policy, e.g. disabling legacy protocols) defaults
            // to $sysconfdir/gnutls/config → baked
            // /opt/ffbuild/etc/gnutls/config; the user machine's policy
            // file is never read (absence tolerated, but silently
            // ignored). Point at the OS-standard spot; the cert store
            // is already probed to absolute paths at configure time
            if e.os == crate::sys::Os::Mac {
                "--with-system-priority-file=/opt/homebrew/etc/gnutls/config"
            } else {
                "--with-system-priority-file=/etc/gnutls/config"
            },
        ],
    )
}
