//! nettle 配方：nettle+hogweed（gnutls 依赖）
//! nettle recipe: nettle+hogweed (gnutls dependency)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("nettle", "linux,mac", "nettle,hogweed", "-", body);
    x.src = include_str!("nettle.rs");
    x
}

/// 构建 nettle+hogweed / Build nettle+hogweed
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 不加 --enable-mini-gmp：真 gmp 已保证，mini-gmp 会在 libhogweed 里
    // 嵌重复的 gmp_* 符号 / no --enable-mini-gmp: the real gmp is
    // guaranteed; mini-gmp embeds duplicate gmp_* symbols in libhogweed
    buildsys::refresh_config_scripts(&src)?;
    let args = [
        "./configure",
        &format!("--prefix={}", e.prefix),
        &format!("--libdir={}/lib", e.prefix),
        "--disable-shared",
        "--enable-static",
        "--with-pic",
        "--disable-openssl",
        "--disable-documentation",
    ];
    sys::bash_ok(&args.join(" "), &src)?;
    // nettle 的 configure 探测 CCPIC 时偏好列表先选 -fpic（64KiB GOT）。
    // aarch64 上约 150 个静态库的最终 PIE 链接会溢出它——强制无上限模型。
    // nettle's configure prefers -fpic (64KiB GOT) for CCPIC; a final PIE
    // link against ~150 static archives overflows it on aarch64 — force
    // the unlimited model.
    for pat in ["config.m4", "config.make", "config.mk", "Makefile"] {
        let f = src.join(pat);
        if f.is_file() {
            fsx::sed_i("s/^\\(CCPIC[[:space:]]*=.*\\)-fpic/\\1-fPIC/", &f, &src)?;
        }
    }
    for f in fsx::walk(&src) {
        if f.file_name().map(|n| n == "Makefile").unwrap_or(false) && f != src.join("Makefile") {
            fsx::sed_i("s/^\\(CCPIC[[:space:]]*=.*\\)-fpic/\\1-fPIC/", &f, &src)?;
        }
    }
    sys::run_ok("make", &["-j", &e.jobs.to_string()], &src)?;
    sys::run_ok("make", &["install"], &src)
}
