//! xavs2 配方：AVS2 编码器（旧 50-video.sh 移植）
//! xavs2 recipe: AVS2 encoder (port of 50-video.sh)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "xavs2",
        "linux:x86_64,windows:x86_64",
        "xavs2",
        "--enable-libxavs2",
        body,
    );
    x.src = include_str!("xavs2.rs");
    x
}

/// 构建 xavs2 / Build xavs2
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let b = src.join("build/linux");
    let cf = b.join("configure");
    let text = fsx::read(&cf)
        .replace("EGIB", "bss")
        .replace("naidnePF", "bss");
    fsx::write(&cf, &text)?;
    sys::run_ok(
        "./configure",
        &[
            "--disable-cli",
            "--enable-static",
            "--enable-pic",
            "--disable-avs",
            "--disable-swscale",
            "--disable-lavf",
            "--disable-ffms",
            "--disable-gpac",
            "--disable-lsmash",
            "--extra-asflags=-w-macro-params-legacy",
            "--extra-cflags=-Wno-error=incompatible-pointer-types",
            &format!("--prefix={}", e.prefix),
        ],
        &b,
    )?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &b)?;
    sys::run_ok("make", &["install"], &b)?;
    // 与 davs2 同款 version.sh 怪癖：无 tag 检出产生 "1.7."
    // same version.sh quirk as davs2: a tagless checkout yields "1.7."
    let pc = e.prefix_win.join("lib/pkgconfig/xavs2.pc");
    if pc.is_file() {
        let _ = fsx::sed_i("s/^(Version:[[:space:]]*[0-9.]+)\\.$/\\1.0/", &pc, &e.work);
    }
    Ok(())
}
