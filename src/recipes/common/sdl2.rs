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
            // 全静态二进制里的 dlopen 是双 libc 地雷：SDL 默认
            // *_SHARED=ON 把后端做成运行时 dlopen 系统 .so——
            // 1) find_library 解析到本 prefix 的 .a 时直接把归档名
            //    烘成 soname（实测 ffplay 里 libX11.a/libpulse.a），
            //    dlopen 永远失败 → 后端先天死亡；
            // 2) 烘成真 soname 的后端（libasound.so.2/GL/udev/dbus）
            //    在用户机上载系统库 → 拖入第二份 libc.so.6/libstdc++
            //    → SIGSEGV/SIGFPE（死相随版本骰子随机）。
            // 全部改直链：在库者静态并入 libSDL2.a，缺库后端自动
            // 禁用；ffplay 走 X11/wayland(无→xwayland)+pulse，
            // GL/vulkan 本就走软件渲染回退，一并关掉断 mesa→DRI→
            // libstdc++ 链 / dlopen in a fully-static binary is a
            // dual-libc landmine: SDL's default *_SHARED=ON dlopens
            // system .so's at runtime — (1) when find_library
            // resolves our prefix's .a it bakes the archive NAME as
            // the soname (observed libX11.a/libpulse.a inside ffplay)
            // which can never dlopen → backend born dead; (2) real
            // sonames (libasound.so.2/GL/udev/dbus) pull a second
            // libc.so.6/libstdc++ into the process on user machines
            // → SIGSEGV/SIGFPE depending on version dice. Force
            // direct linkage: found libs fold into libSDL2.a,
            // missing-lib backends auto-disable; ffplay uses
            // X11 (+xwayland) & pulse; GL/vulkan already software-
            // fallback, disable to sever the mesa→DRI→libstdc++ chain
            "-DSDL_X11_SHARED=OFF",
            "-DSDL_WAYLAND_SHARED=OFF",
            "-DSDL_WAYLAND_LIBDECOR_SHARED=OFF",
            "-DSDL_KMSDRM_SHARED=OFF",
            "-DSDL_ALSA_SHARED=OFF",
            "-DSDL_PULSEAUDIO_SHARED=OFF",
            "-DSDL_PIPEWIRE_SHARED=OFF",
            "-DSDL_JACK_SHARED=OFF",
            "-DSDL_SNDIO_SHARED=OFF",
            "-DSDL_ESD_SHARED=OFF",
            "-DSDL_ARTS_SHARED=OFF",
            "-DSDL_NAS_SHARED=OFF",
            "-DSDL_FUSIONSOUND_SHARED=OFF",
            "-DSDL_LIBSAMPLERATE_SHARED=OFF",
            "-DSDL_OPENGL=OFF",
            "-DSDL_OPENGLES=OFF",
            "-DSDL_VULKAN=OFF",
            // dbus/ibus/fcitx/udev 也是 dlopen 系统库（libdbus→
            // libsystemd 链尤深），ffplay 无需屏保抑制/IME/手柄热插
            // dbus/ibus/fcitx/udev likewise dlopen system libs (the
            // libdbus→libsystemd chain is especially deep); ffplay
            // needs no screensaver-inhibit/IME/gamepad-hotplug
            "-DSDL_DBUS=OFF",
            "-DSDL_IBUS=OFF",
            "-DSDL_LIBUDEV=OFF",
        ],
    )
}
