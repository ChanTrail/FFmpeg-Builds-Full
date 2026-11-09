//! pulseaudio 配方：libpulse 客户端库（libpulse 特性的本体）
//! pulseaudio recipe: the libpulse client library (the body of the
//! libpulse feature)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("pulseaudio", "linux", "libpulse", "-", body);
    x.src = include_str!("pulseaudio.rs");
    x
}

/// 构建 libpulse 静态库（纯客户端）/ Build libpulse statically
/// (client-only)
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // auto_features=disabled 一把关掉全部自动探测依赖（dbus/systemd/
    // udev/bluez5/gtk…），daemon=false 只产客户端：libpulse.a +
    // libpulsecommon-*.a，后者经 libpulse.pc 的 Libs.private 随静态链
    // 带入。运行时仍是 IPC 客户端——连不连得上服务器与链接形态无关 /
    // auto_features=disabled switches off every auto-detected dep
    // (dbus/systemd/udev/bluez5/gtk…); daemon=false yields client only:
    // libpulse.a + libpulsecommon-*.a, the latter pulled along by
    // libpulse.pc's Libs.private in a static link. It's still an IPC
    // client at runtime — server reachability has nothing to do with
    // the link form
    buildsys::meson_build(
        &src,
        &[
            "-Dauto_features=disabled",
            "-Ddaemon=false",
            // v17.0 的选项型谱：doxygen/tests/man/daemon 全是布尔
            // （=disabled 是 feature 型的写法，传它判型不符出局——HEAD
            // 恰好相反，钉 tag 前被 HEAD 的选项表骗过一轮）；alsa/dbus/
            // x11/glib 等 feature 型已被 auto_features=disabled 罩住 /
            // v17.0's option-type map: doxygen/tests/man/daemon are all
            // booleans (=disabled is the feature-type spelling and fails
            // the type check — HEAD is the exact opposite; the HEAD
            // option table fooled one round before the tag pin); feature
            // options like alsa/dbus/x11/glib are already blanketed by
            // auto_features=disabled
            "-Ddoxygen=false",
            "-Dman=false",
            "-Dtests=false",
        ],
    )
}
