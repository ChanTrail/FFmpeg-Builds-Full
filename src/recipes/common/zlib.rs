//! zlib 配方：压缩库（aribb24/libpng 依赖；Linux/Windows 走系统包）
//! zlib recipe: the compression library (aribb24/libpng dependency;
//! Linux/Windows get it from system packages)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("zlib", "linux,mac", "zlib", "-", body);
    x.src = include_str!("zlib.rs");
    x
}

/// 构建 zlib / Build zlib
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    buildsys::cmake_build(&src, &[])?;
    // zlib 的 CMakeLists 无条件同产 shared+static（BUILD_SHARED_LIBS=OFF
    // 在 UNIX 上无效）——dylib 落进前缀后 -lz 全解析到 @rpath 的
    // libz.1.dylib，运行时找不到直接崩（libcurl 探测/librsvg 的
    // cargo-cbuild 连环死因）。删到只剩静态归档 / zlib's CMakeLists
    // emits shared+static unconditionally (BUILD_SHARED_LIBS=OFF is a
    // no-op on UNIX) — the dylib in the prefix makes every -lz resolve to
    // an @rpath libz.1.dylib that dyld can't find at run time (the
    // libcurl probe / librsvg's cargo-cbuild chain). Keep the archive only.
    fsx::rm_glob(&e.prefix_win.join("lib"), "libz*.dylib*");
    Ok(())
}
