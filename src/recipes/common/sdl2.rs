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
    // SDL 的 pc 生成 bug：sdlchecks.cmake 把 find_library 的全路径
    // 塞进 EXTRA_LIBS（SDL_*_SHARED=OFF 直链后端时 X11/Xext 正走
    // 这一路），而 listtostr 对每个元素无条件加 -l 前缀 → sdl2.pc
    // 流出 "-l/opt/ffbuild/lib/libXext.a" 畸形旗标，static_libs_ok
    // 与一切下游链接者必死。按语义重发：绝对路径与已带 - 前缀的项
    // 原样通过，裸名才补 -l / SDL's pc generation bug: sdlchecks.cmake
    // stuffs find_library's absolute paths into EXTRA_LIBS (X11/Xext
    // take exactly that path with *_SHARED=OFF direct linkage), and
    // listtostr unconditionally prepends -l to every element →
    // sdl2.pc emits the malformed "-l/opt/ffbuild/lib/libXext.a",
    // fatal to static_libs_ok and every downstream linker. Re-emit
    // by semantics: absolute paths and dash-prefixed items pass
    // through verbatim, bare names get -l
    let cml = src.join("CMakeLists.txt");
    if cml.is_file() {
        let orig = crate::fsx::read(&cml);
        let mut s = orig.clone();
        // 正序与逆序两个调用点都在发 -l 前缀——2.0.22 起并存 /
        // both the forward and reversed emit sites prefix -l — they
        // coexist since 2.0.22
        let old = "listtostr(EXTRA_LIBS _EXTRA_LIBS \"-l\")";
        if s.contains(old) {
            let new = concat!(
                "set(_EXTRA_LIBS \"\")\n",
                "foreach(_SDL_EXLIB ${EXTRA_LIBS})\n",
                "  if(IS_ABSOLUTE \"${_SDL_EXLIB}\" OR \"${_SDL_EXLIB}\" MATCHES \"^-\")\n",
                "    set(_EXTRA_LIBS \"${_EXTRA_LIBS} ${_SDL_EXLIB}\")\n",
                "  else()\n",
                "    set(_EXTRA_LIBS \"${_EXTRA_LIBS} -l${_SDL_EXLIB}\")\n",
                "  endif()\n",
                "endforeach()"
            );
            s = s.replace(old, new);
        }
        let old = "listtostrrev(EXTRA_LIBS _EXTRA_LIBS \"-l\")";
        if s.contains(old) {
            let new = concat!(
                "set(_EXTRA_LIBS \"\")\n",
                "foreach(_SDL_EXLIB ${EXTRA_LIBS})\n",
                "  if(IS_ABSOLUTE \"${_SDL_EXLIB}\" OR \"${_SDL_EXLIB}\" MATCHES \"^-\")\n",
                "    set(_EXTRA_LIBS \"${_SDL_EXLIB} ${_EXTRA_LIBS}\")\n",
                "  else()\n",
                "    set(_EXTRA_LIBS \"-l${_SDL_EXLIB} ${_EXTRA_LIBS}\")\n",
                "  endif()\n",
                "endforeach()"
            );
            s = s.replace(old, new);
        }
        if s != orig {
            crate::fsx::write(&cml, &s)?;
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
