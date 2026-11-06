//! davs2 配方：AVS2 解码器（旧 50-video.sh 移植）
//! davs2 recipe: AVS2 decoder (port of 50-video.sh)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "davs2",
        "linux:x86_64,windows:x86_64",
        "davs2",
        "--enable-libdavs2",
        body,
    );
    x.src = include_str!("davs2.rs");
    x
}

/// 构建 davs2 / Build davs2
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let b = src.join("build/linux");
    // configure 的字节序探测在现代 binutils 下会失败；目标平台全是小端
    // the configure endianness probe fails with modern binutils;
    // everything we target is little endian
    let cf = b.join("configure");
    let text = fsx::read(&cf)
        .replace("EGIB", "bss")
        .replace("naidnePF", "bss");
    fsx::write(&cf, &text)?;
    sys::run_ok(
        "./configure",
        &[
            "--disable-cli",
            "--enable-pic",
            &format!("--prefix={}", e.prefix),
        ],
        &b,
    )?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &b)?;
    sys::run_ok("make", &["install"], &b)?;
    // 无 tag 检出时 version.sh 输出 "1.6."——非法 pkg-config 版本号，让
    // FFmpeg 的 "davs2 >= 1.6.0" 失败 / their version.sh emits "1.6."
    // tagless — not a valid pkg-config version and fails FFmpeg's check
    let pc = e.prefix_win.join("lib/pkgconfig/davs2.pc");
    fsx::sed_i("s/^(Version:[[:space:]]*[0-9.]+)\\.$/\\1.0/", &pc, &e.work)?;
    Ok(())
}
