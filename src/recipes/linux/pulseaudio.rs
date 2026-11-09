//! pulseaudio 配方：libpulse 客户端库（libpulse 特性的本体）
//! pulseaudio recipe: the libpulse client library (the body of the
//! libpulse feature)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("pulseaudio", "linux", "libpulse", "-", body);
    x.src = include_str!("pulseaudio.rs");
    x
}

/// meson.build 里 `shared_library(` 全转 `library(` 并摘掉 static
/// 目标不认的 kwargs。v17.0 把 libpulse/libpulse-simple/libpulse-
/// mainloop-glib/libpulsecommon 全部写死 shared_library()——
/// `--default-library=static` 只罩 library() 裸调用，四本 .so 照产，
/// ffmpeg -lpulse 只能动态链（实证 ldd 白名单抓出 libpulse.so.0 +
/// libpulsecommon-17.0.so 整串）。转 library() 后随 default_library
/// 落 .a。version/install_rpath/vs_module_defs 在 static 目标上判型
/// 不符直接出局，逐行删 / swap every `shared_library(` in meson.build
/// for `library(` and strip kwargs static targets reject. v17.0 hard-
/// codes shared_library() for libpulse/libpulse-simple/libpulse-
/// mainloop-glib/libpulsecommon — `--default-library=static` only
/// covers bare library() calls, so all four .so still shipped and
/// ffmpeg's -lpulse could only bind dynamically (empirical: the ldd
/// allowlist caught libpulse.so.0 + libpulsecommon-17.0.so). After the
/// swap they land as .a under default_library. version/install_rpath/
/// vs_module_defs fail the type check on static targets — strip those
/// lines
fn de_share(src: &std::path::Path) -> Result<(), String> {
    for rel in ["src/meson.build", "src/pulse/meson.build"] {
        let p = src.join(rel);
        let s = fsx::read(&p);
        let s = s.replace("shared_library(", "library(");
        let s: String = s
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !(t.starts_with("version :")
                    || t.starts_with("soversion :")
                    || t.starts_with("install_rpath :")
                    || t.starts_with("vs_module_defs :"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        fsx::write(&p, &s)?;
    }
    Ok(())
}

/// 构建 libpulse 静态库（纯客户端）/ Build libpulse statically
/// (client-only)
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    de_share(&src)?;
    // daemon=false 只产客户端：libpulse.a + libpulsecommon-*.a，后者经
    // libpulse.pc 的 Libs.private 随静态链带入。运行时仍是 IPC 客户端
    // ——连不连得上服务器与链接形态无关。可选依赖全部显式 disabled/
    // false 双保险：auto_features 只罩 feature 型且 required:false 的
    // 普通可选依赖不受其管辖，实证 libdbus/libsystemd/libx11/
    // libasyncns 还是进了 libpulsecommon 的 NEEDED / daemon=false
    // yields client only: libpulse.a + libpulsecommon-*.a, the latter
    // pulled along by libpulse.pc's Libs.private in a static link. It's
    // still an IPC client at runtime — server reachability has nothing
    // to do with the link form. Every optional dep gets an explicit
    // disabled/false as belt-and-suspenders: auto_features only governs
    // feature-type options while plain required:false optionals ignore
    // it — empirically libdbus/libsystemd/libx11/libasyncns still made
    // libpulsecommon's NEEDED list
    buildsys::meson_build(
        &src,
        &[
            "-Dauto_features=disabled",
            "-Ddaemon=false",
            // v17.0 的选项型谱：doxygen/tests/man/daemon 全是布尔
            // （=disabled 是 feature 型的写法，传它判型不符出局——HEAD
            // 恰好相反，钉 tag 前被 HEAD 的选项表骗过一轮）/
            // v17.0's option-type map: doxygen/tests/man/daemon are all
            // booleans (=disabled is the feature-type spelling and fails
            // the type check — HEAD is the exact opposite; the HEAD
            // option table fooled one round before the tag pin)
            "-Ddoxygen=false",
            "-Dman=false",
            "-Dtests=false",
            // feature 型可选依赖全禁 / feature-type optional deps all off
            "-Dalsa=disabled",
            "-Dasyncns=disabled",
            "-Davahi=disabled",
            "-Dbluez5=disabled",
            "-Dbluez5-gstreamer=disabled",
            "-Dbluez5-native-headset=disabled",
            "-Dconsolekit=disabled",
            "-Ddbus=disabled",
            "-Delogind=disabled",
            "-Dfftw=disabled",
            "-Dglib=disabled",
            "-Dgsettings=disabled",
            "-Dgstreamer=disabled",
            "-Dgtk=disabled",
            "-Dhal-compat=disabled",
            "-Djack=disabled",
            "-Dlirc=disabled",
            "-Dopenssl=disabled",
            "-Dorc=disabled",
            "-Doss-output=disabled",
            "-Dsamplerate=disabled",
            "-Dsoxr=disabled",
            "-Dspeex=disabled",
            "-Dsystemd=disabled",
            "-Dtcpwrap=disabled",
            "-Dudev=disabled",
            "-Dvalgrind=disabled",
            "-Dx11=disabled",
            "-Dwebrtc-aec=disabled",
            // 布尔型可选依赖 / boolean optional deps
            "-Dadrian-aec=false",
            "-Dbluez5-ofono-headset=false",
        ],
    )?;
    // libpulse.pc 的 Libs.private 只有 -lpulsecommon-17.0——sndfile_dep
    // 是硬依赖但 .so 时代靠 NEEDED 动态传递，pkg-config --static 不会
    // 吐 -lsndfile；转 .a 后 sf_* 引用必须显式入链，否则 configure
    // 链测 undefined 丢整个 libpulse / libpulse.pc's Libs.private
    // carries only -lpulsecommon-17.0 — sndfile_dep is a hard dep whose
    // .so-era NEEDED propagated it dynamically, so pkg-config --static
    // never emits -lsndfile; once .a, the sf_* refs must join the link
    // explicitly or the configure test goes undefined and drops all of
    // libpulse
    let e = crate::sys::env();
    let pc = e.prefix_win.join("lib/pkgconfig/libpulse.pc");
    if pc.is_file() {
        let s = fsx::read(&pc);
        if !s.contains("-lsndfile") {
            let out: String = s
                .lines()
                .map(|l| {
                    if l.starts_with("Libs.private:") {
                        format!("{l} -lsndfile")
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let out = if out.contains("-lsndfile") {
                out
            } else {
                format!("{out}\nLibs.private: -lsndfile\n")
            };
            fsx::write(&pc, &out)?;
        }
    }
    Ok(())
}
