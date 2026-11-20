//! libx11 配方：X11 客户端库（xlib 特性的本体）
//! libx11 recipe: the X11 client library (the body of the xlib feature)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libx11", "linux", "x11", "-", body);
    x.src = include_str!("libx11.rs");
    x
}

/// 构建 libX11 静态库 / Build libX11 statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // XErrorDB、XKeysymDB、X11 locale 表都是运行时数据文件——默认
    // $datadir/X11/* 烤 /opt/ffbuild，用户机上 XGetErrorDatabaseText
    // 只能吐 "Unknown error"、XIM 找不到 compose 表。datadir 指系
    // 统标准位（Debian/Fedora/Arch 一致 /usr/share/X11）；同时是安
    // 装目录，经 DESTDIR stage 防写脏构建机（$libdir/X11/locale 的
    // xlibi18n 模块 .so 静态链里本就跑不动，随 stage 丢弃无害）/
    // XErrorDB, XKeysymDB and the X11 locale tables are all runtime
    // data files — the default $datadir/X11/* bakes /opt/ffbuild, so
    // on user machines XGetErrorDatabaseText can only emit "Unknown
    // error" and XIM finds no compose table. Point datadir at the
    // system-standard spot (/usr/share/X11 across Debian/Fedora/
    // Arch); it doubles as the install dir, so stage via DESTDIR to
    // keep the build machine clean (the $libdir/X11/locale xlibi18n
    // .so modules can't load in a static link anyway — dropping that
    // staged copy is harmless)
    buildsys::autotools_build_destdir(
        &src,
        &[
            "--disable-specs",
            "--datadir=/usr/share",
            "--datarootdir=/usr/share",
        ],
        &[],
        &src.join("x11stage"),
    )
}
