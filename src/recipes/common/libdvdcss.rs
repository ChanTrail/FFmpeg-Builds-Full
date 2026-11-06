//! libdvdcss 配方：DVD 解扰（1.6.0 只有 meson；旧 60-media-io.sh 移植）
//! libdvdcss recipe: DVD descrambling, 1.6.0 ships meson only (port of
//! 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys::{self, Os};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libdvdcss", "mac", "libdvdcss", "-", body);
    x.src = include_str!("libdvdcss.rs");
    x
}

/// 构建 libdvdcss / Build libdvdcss
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    buildsys::meson_build(&src, &[])?;
    // Darwin 上 device.c 调 IOKit/CoreFoundation，上游 .pc 从不在
    // Libs.private 列这些 framework——静态消费者报未定义 / on Darwin
    // device.c talks to IOKit/CoreFoundation but the .pc never lists the
    // frameworks — static consumers fail with undefined symbols
    let pc = e.prefix_win.join("lib/pkgconfig/libdvdcss.pc");
    if e.os == Os::Mac && pc.is_file() && !fsx::contains(&pc, "framework") {
        pkgcfg::libs_private(
            "libdvdcss",
            &["-Wl,-framework,CoreFoundation", "-Wl,-framework,IOKit"],
        );
    }
    Ok(())
}
