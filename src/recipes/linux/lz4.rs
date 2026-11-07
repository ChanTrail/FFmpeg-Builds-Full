//! lz4 配方：静态库 + liblz4.pc（被 libtiff 的 Libs.private 拖入）
//! lz4 recipe: static lib + liblz4.pc (pulled in by libtiff's
//! Libs.private)

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("lz4", "linux", "liblz4", "-", body);
    x.src = include_str!("lz4.rs");
    x
}

/// 构建 liblz4 静态库并生成 liblz4.pc
/// Build liblz4 statically and generate liblz4.pc
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 只要静态库；FFmpeg 只需要能解析 -llz4 / static archive only;
    // FFmpeg just needs a resolvable -llz4
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-C", "lib", "-j", &jobs, "liblz4.a"], &src)?;
    fsx::install(
        &src.join("lib/liblz4.a"),
        &e.prefix_win.join("lib/liblz4.a"),
    )?;
    for h in ["lz4.h", "lz4hc.h", "lz4frame.h"] {
        fsx::install(
            &src.join("lib").join(h),
            &e.prefix_win.join("include").join(h),
        )?;
    }
    fsx::write(
        &e.prefix_win.join("lib/pkgconfig/liblz4.pc"),
        &format!(
            "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\nName: liblz4\nDescription: extremely fast lossless compression\nVersion: 1.10.0\nLibs: -L${{libdir}} -llz4\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )
}
