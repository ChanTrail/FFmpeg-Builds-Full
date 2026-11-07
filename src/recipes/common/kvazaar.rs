//! kvazaar 配方：HEVC 编码器
//! kvazaar recipe: HEVC encoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "kvazaar",
        "linux,mac",
        "kvazaar",
        "--enable-libkvazaar",
        body,
    );
    x.src = include_str!("kvazaar.rs");
    x
}

/// 构建 kvazaar / Build kvazaar
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    sys::run_ok("autoreconf", &["-fiv"], &src)?;
    // --disable-werror 到不了 makefile 里每个 -Werror（clang）
    // --disable-werror does not reach every -Werror the makefiles emit
    let _g = sys::EnvGuard::append(&[("CFLAGS", "-Wno-error")]);
    buildsys::autotools_build(&src, &["--disable-werror"])
}
