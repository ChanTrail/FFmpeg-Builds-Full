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
            // doxygen/tests 是 feature 型选项（值域 enabled/disabled/
            // auto）——传 =false 直接判型不符出局；man/daemon 才是布尔 /
            // doxygen and tests are feature-type options (enabled/
            // disabled/auto) — '=false' is a type error; only 'man' and
            // 'daemon' are booleans
            "-Ddoxygen=disabled",
            "-Dman=false",
            "-Dtests=disabled",
        ],
    )
}
