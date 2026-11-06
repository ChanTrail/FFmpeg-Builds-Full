//! alsa 配方：Linux 音频采集（旧 70-linux-hw.sh 移植）
//! alsa recipe: Linux audio capture (port of 70-linux-hw.sh)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("alsa", "linux", "alsa", "--enable-alsa", body);
    x.src = include_str!("alsa.rs");
    x
}

/// 构建 alsa-lib 静态库 / Build alsa-lib statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // alsa-lib 静态构建不接受 --with-pic；目标文件仍通过全局 CFLAGS 得到
    // PIC，只需去掉 libtool 开关 / alsa-lib refuses --with-pic for a static
    // build; objects are still PIC via global CFLAGS
    sys::run_ok(
        "./configure",
        &[
            &format!("--prefix={}", e.prefix),
            &format!("--libdir={}/lib", e.prefix),
            "--disable-shared",
            "--enable-static",
            "--disable-python",
            "--disable-topology",
            "--disable-ucm",
            "--disable-alisp",
            "--disable-old-symbols",
            "--disable-seq",
            "--disable-rawmidi",
            "--disable-hwdep",
            "--disable-aload",
        ],
        &src,
    )?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &src)?;
    sys::run_ok("make", &["install"], &src)
}
