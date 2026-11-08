//! dc1394 配方：1394 摄像机控制
//! dc1394 recipe: 1394 camera control

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
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
    // 上游 configure 对 libusb-1.0 只会"找到就开"，没有 --without-usb：
    // 检出后 control.c/capture.c 的 #ifdef HAVE_LIBUSB 块烙上 libusb_*
    // 引用，而 libdc1394-2.pc 的 Libs.private 又不带 -lusb-1.0——FFmpeg
    // 静态链必炸（undefined reference）。环境变量路径走不通：sys::cmd
    // 对每个子进程强灌 PKG_CONFIG_LIBDIR（linux 版刻意并入系统 .pc 目
    // 录保 x11/glib 可见），配方侧 set_var 会被整体覆盖。
    // 换成 PKG_CONFIG 包装器精确否决：configure 接受 VAR=value 命令行
    // 参数，PKG_CHECK_MODULES 又以 $PKG_CONFIG 为名调用——凡参数提及
    // libusb-1.0 一律报失败，其余查询原样透传（raw1394 等不受影响），
    // USB 层从此不编。
    // Upstream configure only auto-detects libusb-1.0 (there is no
    // --without-usb): once found, the #ifdef HAVE_LIBUSB blocks in
    // control.c/capture.c bake libusb_* references in, while
    // libdc1394-2.pc's Libs.private carries no -lusb-1.0 — FFmpeg's
    // static link then dies (undefined references). The env-var path is
    // a dead end: sys::cmd force-feeds PKG_CONFIG_LIBDIR into every
    // child (on linux it deliberately folds in the system .pc dir to
    // keep x11/glib visible), which overrides anything a recipe sets.
    // Use a PKG_CONFIG wrapper for a targeted denial instead: configure
    // accepts VAR=value command-line arguments, and PKG_CHECK_MODULES
    // invokes whatever $PKG_CONFIG names — any call mentioning
    // libusb-1.0 fails while every other query passes through verbatim
    // (raw1394 & co. unaffected), so the USB layer is never compiled.
    let wrap = e.work.join("pc-deny-libusb.sh");
    fsx::write(
        &wrap,
        "#!/bin/sh\n\
         # libusb-1.0 查询一律失败，其余透传 / fail libusb-1.0 queries,\n\
         # pass everything else through verbatim\n\
         for a in \"$@\"; do\n\
         \tcase \"$a\" in *libusb-1.0*) exit 1;; esac\n\
         done\n\
         exec pkg-config \"$@\"\n",
    )?;
    sys::run_ok("chmod", &["+x", &wrap.to_string_lossy()], &e.work)?;
    let w = wrap.to_string_lossy().to_string();
    buildsys::autotools_build(
        &src,
        &[
            "--disable-examples",
            "--disable-doxygen-doc",
            "--disable-doxygen-html",
            "--disable-doxygen-dot",
            &format!("PKG_CONFIG={w}"),
        ],
    )
}
