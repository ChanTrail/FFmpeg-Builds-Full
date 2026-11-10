//! sdl2 配方：SDL2（ffplay/sdl 输出设备用）
//! sdl2 recipe: SDL2 (ffplay/sdl output devices)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("sdl2", "linux,mac", "sdl2", "--enable-sdl2", body);
    x.src = include_str!("sdl2.rs");
    x
}

/// 构建 SDL2 / Build SDL2
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // libX11 ≥1.8 在 Xlib.h 自带 XGenericEventCookie typedef（匿名
    // 结构体）。libXi/libXfixes 缺席时 SDL 关掉 xinput2/xfixes，
    // SDL_VIDEO_DRIVER_X11_SUPPORTS_GENERIC_EVENTS 未定义，xinput2.h
    // 为让声明可编译补发的"具名结构体 typedef"与之撞型（具名 vs
    // 匿名是真冲突，C11 re-typedef 豁免不适用）。把补发的两行换成
    // 直接引 Xlib.h——任一包含序都只见到 Xlib 的那一个定义（头文件
    // 自带 include guard 幂等）/ libX11 ≥1.8 typedefs
    // XGenericEventCookie in Xlib.h (anonymous struct). With
    // libXi/libXfixes absent SDL disables xinput2/xfixes,
    // SDL_VIDEO_DRIVER_X11_SUPPORTS_GENERIC_EVENTS stays undefined
    // and the tagged-struct typedef xinput2.h emits so its
    // declarations compile collides with it (tagged vs anonymous is a
    // real conflict — the C11 re-typedef exemption doesn't apply).
    // Swap the fallback two lines for a direct Xlib.h include — either
    // include order sees exactly Xlib's one definition (the header's
    // own include guard makes it idempotent)
    let xh = src.join("src/video/x11/SDL_x11xinput2.h");
    if xh.is_file() {
        let s = crate::fsx::read(&xh);
        let old =
            "struct XGenericEventCookie;\ntypedef struct XGenericEventCookie XGenericEventCookie;";
        if s.contains(old) {
            crate::fsx::write(&xh, &s.replace(old, "#include <X11/Xlib.h>"))?;
        }
    }
    buildsys::cmake_build(
        &src,
        &[
            "-DSDL_SHARED=OFF",
            "-DSDL_STATIC=ON",
            "-DSDL_TEST=OFF",
            "-DSDL_TESTS=OFF",
        ],
    )
}
