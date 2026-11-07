//! caca 配方：彩色 ASCII 艺术渲染
//! caca recipe: colour ASCII art rendering

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("caca", "linux,mac", "caca", "--enable-libcaca", body);
    x.src = include_str!("caca.rs");
    x
}

/// 构建 libcaca / Build libcaca
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 自带的 cacaview 调 _caca_alloc2d 没原型；隐式 int 撞 int-conversion，
    // GCC 14+/Clang 16+ 默认下是硬错误 / bundled cacaview calls
    // _caca_alloc2d without a prototype; implicit-int then hits
    // int-conversion, a hard error under GCC 14+/Clang 16+ defaults
    let _g = sys::EnvGuard::append(&[(
        "CFLAGS",
        "-Wno-implicit-function-declaration -Wno-error=int-conversion",
    )]);
    buildsys::autotools_build(
        &src,
        &[
            "--disable-doc",
            "--disable-ruby",
            "--disable-csharp",
            "--disable-java",
            "--disable-python",
            "--disable-cxx",
            "--disable-x11",
            "--disable-gl",
            "--disable-ncurses",
            "--disable-slang",
            "--disable-imlib2",
            "--disable-cocoa",
            "--disable-conio",
            "--disable-win32",
            "--disable-vga",
        ],
    )
}
