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
            "--enable-x11",
            "--enable-drm",
            "--disable-wayland",
            "--disable-glx",
            "--disable-docs",
        ],
    )
}
