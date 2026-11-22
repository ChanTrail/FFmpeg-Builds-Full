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
    // SDL 把 alsa 函数表声明成 int(*)，而 alsa-lib 的
    // snd_pcm_info_free 自引入（1.2.6）起就返回 void——dlopen
    // 时代只是 dlsym 指针不验型，*_SHARED=OFF 直链后宏
    // `ALSA_##x = x` 把 void(*) 赋给 int(*) → GCC14+
    // -Wincompatible-pointer-types 硬错误（arm64-legacy CI 实证）。
    // 两处调用点（505/528）皆语句调用、返回值未用，改 void 安全；
    // 全表逐一对照 alsa-lib 签名，仅此一处错 / SDL declares its
    // alsa function table as int(*), but alsa-lib's
    // snd_pcm_info_free has returned void since its introduction
    // (1.2.6) — under dlopen it was only a dlsym pointer never
    // type-checked; with *_SHARED=OFF direct linkage the macro
    // `ALSA_##x = x` assigns void(*) to int(*) → GCC14+
    // -Wincompatible-pointer-types hard error (arm64-legacy CI
    // evidence). Both call sites (505/528) are statement calls
    // ignoring the return — void is safe; the whole table was
    // cross-checked against alsa-lib signatures, this is the
    // only mismatch
    if crate::sys::env().os == crate::sys::Os::Linux {
        let alsa = src.join("src/audio/alsa/SDL_alsa_audio.c");
        if alsa.is_file() {
            let s = crate::fsx::read(&alsa);
            let old = "static int (*ALSA_snd_pcm_info_free)(snd_pcm_info_t *);";
            if s.contains(old) {
                crate::fsx::write(
                    &alsa,
                    &s.replace(
                        old,
                        "static void (*ALSA_snd_pcm_info_free)(snd_pcm_info_t *);",
                    ),
                )?;
            }
        }
    }
    let mut cmake_args: Vec<&str> = vec![
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
    ];
    // 直链后 check_library_exists 的裸 -l 链测缺传递依赖
    // （libasound.a 要 -lm/-lrt/-ldl 才链接过，缺了 SDL 就把
    // ALSA 后端静默关死——"snd_pcm_recover in asound - not
    // found" 实证）。linux 限定：macOS 无 librt/libdl，挂上反而
    // 让 HAVE_PTHREADS 等一切链测假死 / with direct linkage,
    // check_library_exists' bare -l probes lack transitives
    // (libasound.a needs -lm/-lrt/-ldl to link, without which SDL
    // silently kills the ALSA backend — "snd_pcm_recover in asound
    // - not found", empirical). Linux only: macOS has no
    // librt/libdl — hanging them on makes HAVE_PTHREADS and every
    // other link check die
    let e = crate::sys::env();
    if e.os == crate::sys::Os::Linux {
        cmake_args.push("-DCMAKE_REQUIRED_LIBRARIES=m;dl;rt;pthread");
    }
    buildsys::cmake_build(&src, &cmake_args)?;

    // linux 限定：SDL 把后端裸记号（find_library 全路径 / -l 名）
    // 连同重复两份的 -lSDL2 全塞 Libs:——pkgconf 去重保尾部，把
    // -lSDL2 推到所有私有依赖之后（顺序死，实证：pa_*/X* 全
    // undefined），且后端库的传递链（xcb/pulsecommon）毫无记录
    // （缺链死）。把可映射的后端记号换成 Requires.private 模块：
    // pkg-config --static 拓扑展开整条私有链，天然排在 -lSDL2
    // 之后；-lSDL2 自身去重保首位 / Linux only: SDL stuffs raw
    // backend tokens (find_library full paths / -l names) into
    // Libs: along with a duplicated -lSDL2 — pkgconf dedup keeps
    // the last one, stranding -lSDL2 behind every private dep
    // (order death — pa_*/X* all undefined, empirical), and
    // backend transitives (xcb/pulsecommon) go unrecorded (missing
    // chain death). Swap mappable backend tokens for
    // Requires.private modules: pkg-config --static expands the
    // whole private chain topologically, naturally after -lSDL2;
    // -lSDL2 itself dedups keeping the first spot
    if e.os == crate::sys::Os::Linux {
        let pc = e.prefix_win.join("lib").join("pkgconfig").join("sdl2.pc");
        if pc.is_file() {
            let c = crate::fsx::read(&pc);
            let mut reqs: Vec<&str> = Vec::new();
            let mut out = String::new();
            for line in c.lines() {
                let val = line
                    .strip_prefix("Libs.private:")
                    .or_else(|| line.strip_prefix("Libs:"));
                if let Some(v) = val {
                    let mut toks: Vec<&str> = Vec::new();
                    for t in v.split_whitespace() {
                        // 归档全路径取文件名后再比对 / compare archive
                        // full paths by basename
                        let base = t.rsplit('/').next().unwrap_or(t);
                        let m = match base {
                            "libX11.a" | "-lX11" => Some("x11"),
                            "libXext.a" | "-lXext" => Some("xext"),
                            "libXfixes.a" | "-lXfixes" => Some("xfixes"),
                            "libasound.a" | "-lasound" => Some("alsa"),
                            "libpulse.a" | "-lpulse" => Some("libpulse"),
                            "libpulse-simple.a" | "-lpulse-simple" => Some("libpulse-simple"),
                            _ => None,
                        };
                        match m {
                            Some(m) => {
                                if !reqs.contains(&m) {
                                    reqs.push(m);
                                }
                            }
                            // 裸记号去重保首位——-lSDL2 由此归位 /
                            // dedup remaining tokens keeping first —
                            // this is what reseats -lSDL2
                            None => {
                                if !toks.contains(&t) {
                                    toks.push(t);
                                }
                            }
                        }
                    }
                    out.push_str(if line.starts_with("Libs.private:") {
                        "Libs.private:"
                    } else {
                        "Libs:"
                    });
                    for t in toks {
                        out.push(' ');
                        out.push_str(t);
                    }
                    out.push('\n');
                } else {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            if !reqs.is_empty() {
                out.push_str(&format!("Requires.private: {}\n", reqs.join(" ")));
            }
            crate::fsx::write(&pc, &out)?;
        }
    }
    Ok(())
}
