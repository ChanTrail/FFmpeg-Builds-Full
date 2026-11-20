//! libva 配方：VA-API 库（libva/libva-drm/libva-x11 三件套同树产出）
//! libva recipe: the VA-API library (one build yields the
//! libva/libva-drm/libva-x11 trio)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // 多探针：features 的 vaapi 查 libva.pc，FFmpeg configure 另查
    // libva-drm（vaapi_drm）与 libva-x11（vaapi_x11）——三者全在才算
    // 建成 / multi-probe: the vaapi feature checks libva.pc while FFmpeg
    // configure separately checks libva-drm (vaapi_drm) and libva-x11
    // (vaapi_x11) — all three must exist
    let mut x = r("libva", "linux", "libva,libva-drm,libva-x11", "-", body);
    x.src = include_str!("libva.rs");
    x
}

/// 构建 libva 静态库（x11+drm 后端）/ Build libva statically (x11+drm
/// backends)
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // --disable-wayland 免 wayland-scanner；--disable-glx 免 libGL；
    // --disable-docs 免 doxygen。驱动（iHD/i965 等 .so）本就运行时
    // dlopen，与静态链无冲突 / --disable-wayland avoids wayland-scanner;
    // --disable-glx avoids libGL; --disable-docs avoids doxygen. Drivers
    // (iHD/i965 .so) load via dlopen at runtime anyway — no conflict
    // with static linking
    buildsys::autotools_build(
        &src,
        &[
            // VA 驱动（i965_drv_video/iHD_drv_video.so）运行时 dlopen，
            // 默认 $libdir/dri 把 /opt/ffbuild/lib/dri 烤进库——用户机
            // 无此路径，vaInitialize 直接 VA_STATUS_ERROR→VAAPI 全死。
            // 上游 va_openDriver 对编译期 VA_DRIVERS_PATH 与
            // LIBVA_DRIVERS_PATH env 走同一个 strtok_r(':')——冒号多
            // 目录写进编译期值同样生效，一次盖全 Debian multiarch /
            // Fedora / Arch 三家位 / VA drivers (i965_drv_video/
            // iHD_drv_video.so) dlopen at runtime; the default
            // $libdir/dri bakes /opt/ffbuild/lib/dri — user machines
            // lack it and vaInitialize dies. Upstream va_openDriver
            // runs the compiled VA_DRIVERS_PATH through the same
            // strtok_r(':') as the LIBVA_DRIVERS_PATH env — a colon
            // list works in the baked value too, covering Debian
            // multiarch, Fedora and Arch at once
            "--with-drivers-path=/usr/lib/x86_64-linux-gnu/dri:/usr/lib64/dri:/usr/lib/dri",
            // /etc/libva.conf 是运行时覆写文件（fopen 缺席容忍），指
            // 系统 sysconfdir 而不是前缀 / /etc/libva.conf is a runtime
            // override file (absent-fopen tolerated) — point at the
            // system sysconfdir, not the prefix
            "--sysconfdir=/etc",
            "--enable-x11",
            "--enable-drm",
            "--disable-wayland",
            "--disable-glx",
            "--disable-docs",
        ],
    )
}
