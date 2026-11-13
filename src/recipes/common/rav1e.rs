//! rav1e 配方：Rust AV1 编码器（cargo-c 出 C 库 + .pc）
//! rav1e recipe: the Rust AV1 encoder (cargo-c produces the C library +
//! .pc)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("rav1e", "linux,mac", "rav1e", "--enable-librav1e", body);
    x.src = include_str!("rav1e.rs");
    x
}

/// 构建 rav1e / Build rav1e
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // cargo-c 是 cargo 子命令，缺席就现装（runner 自带 stable 工具链）
    // cargo-c is a cargo subcommand — install it on demand (the runner
    // already has a stable toolchain)
    if sys::which("cargo-cinstall").is_none() {
        sys::run_ok("cargo", &["install", "cargo-c", "--locked"], &ctx.wd)?;
    }
    sys::run_ok(
        "cargo",
        &[
            "cinstall",
            "--release",
            "--prefix",
            &e.prefix,
            "--libdir",
            &format!("{}/lib", e.prefix),
            "--library-type=staticlib",
        ],
        &src,
    )
}
