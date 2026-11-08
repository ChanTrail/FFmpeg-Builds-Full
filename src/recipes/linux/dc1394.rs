//! dc1394 配方：1394 摄像机控制
//! dc1394 recipe: 1394 camera control

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("dc1394", "linux", "libdc1394-2", "--enable-libdc1394", body);
    x.src = include_str!("dc1394.rs");
    x
}

/// 构建 libdc1394 / Build libdc1394
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 上游 configure 对 libusb-1.0 只会"找到就开"，没有 --disable-usb：
    // 检出后把 dc1394/usb 一层编进 .a 烙上 libusb_* 引用，而上
    // libdc1394-2.pc 的 Libs.private 又不带 -lusb-1.0——FFmpeg 静态链
    // 必炸（lt6-control.o/lt5-capture.o undefined reference）。把
    // pkg-config 搜索域临时掐到前缀内：系统侧的 libusb-1.0.pc 从此找
    // 不到 → USB 层整个不编（FFmpeg 只要火线栈）；raw1394.pc 在前缀
    // 照常解析。
    // Upstream configure only auto-detects libusb-1.0 (there is no
    // --disable-usb): once found, dc1394/usb gets compiled into the .a
    // with libusb_* references, while libdc1394-2.pc's Libs.private
    // carries no -lusb-1.0 — FFmpeg's static link then dies
    // (lt6-control.o/lt5-capture.o undefined references). Temporarily
    // narrow the pkg-config search domain to the prefix: the system's
    // libusb-1.0.pc becomes invisible, so the USB layer is never built
    // (FFmpeg only needs the FireWire stack); raw1394.pc still resolves
    // from the prefix.
    let pc_dir = e.prefix_win.join("lib/pkgconfig");
    let saved_path = std::env::var("PKG_CONFIG_PATH").ok();
    let saved_libdir = std::env::var("PKG_CONFIG_LIBDIR").ok();
    // SAFETY：配方体在引擎单线程循环内执行 / SAFETY: recipe bodies run
    // inside the engine's single-threaded loop
    unsafe {
        std::env::set_var("PKG_CONFIG_PATH", &pc_dir);
        std::env::set_var("PKG_CONFIG_LIBDIR", &pc_dir);
    }
    let ret = buildsys::autotools_build(
        &src,
        &[
            "--disable-examples",
            "--disable-doxygen-doc",
            "--disable-doxygen-html",
            "--disable-doxygen-dot",
        ],
    );
    // 还原搜索域，别给后续配方留暗雷 / restore the search domain — no
    // landmines for later recipes
    unsafe {
        match saved_path {
            Some(v) => std::env::set_var("PKG_CONFIG_PATH", v),
            None => std::env::remove_var("PKG_CONFIG_PATH"),
        }
        match saved_libdir {
            Some(v) => std::env::set_var("PKG_CONFIG_LIBDIR", v),
            None => std::env::remove_var("PKG_CONFIG_LIBDIR"),
        }
    }
    ret
}
