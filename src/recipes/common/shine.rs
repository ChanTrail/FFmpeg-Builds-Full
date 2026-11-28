//! shine 配方：定点 MP3 编码器
//! shine recipe: fixed-point MP3 encoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("shine", "linux,mac", "shine", "--enable-libshine", body);
    x.src = include_str!("shine.rs");
    x
}

/// 构建 libshine / Build libshine
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    sys::run_ok("autoreconf", &["-fiv"], &src)?;
    // K&R 时代的声明与 C23 的 `()` 冲突，必须钉前 C23 的标准；但
    // gnu89 不行——shine 4.0.0 的 CLI 前端（bin/wave.c、bin/main.c）
    // 已现代化到 for 循环内声明（C99 特性），gnu89 判死。gnu99 是
    // 两代交集：K&R 函数定义合法保留、`()` 保留旧义，且 for 循环
    // 内声明可用 / K&R-era declarations collide with C23's `()`, so
    // a pre-C23 standard must be pinned; but gnu89 won't do — shine
    // 4.0.0's CLI frontend (bin/wave.c, bin/main.c) modernized to
    // in-for-loop declarations (a C99 feature) and gnu89 kills it.
    // gnu99 is the intersection of both eras: K&R function
    // definitions stay legal, `()` keeps its legacy meaning, and
    // in-for-loop declarations work
    let _g = sys::EnvGuard::append(&[("CFLAGS", "-std=gnu99")]);
    buildsys::autotools_build(&src, &[])
}
