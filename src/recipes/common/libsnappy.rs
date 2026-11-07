//! snappy 配方：Google 快速压缩（SVT-AV1/若干封装用）
//! snappy recipe: Google's fast compression (SVT-AV1/muxers use it)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libsnappy",
        "linux,mac",
        "snappy",
        "--enable-libsnappy",
        body,
    );
    x.src = include_str!("libsnappy.rs");
    x
}

/// 构建 snappy / Build snappy
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = crate::sys::env();
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &["-DSNAPPY_BUILD_TESTS=OFF", "-DSNAPPY_BUILD_BENCHMARKS=OFF"],
    )?;
    // 上游 cmake 只装库+头，不产 snappy.pc——pkg-config 探针全靠手写
    // 的这个 / upstream cmake installs lib+headers but no snappy.pc —
    // the pkg-config probe relies on this hand-written one
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/snappy.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: snappy\nDescription: Fast compression/decompression library\nVersion: 1.2.2\nLibs: -L${{libdir}} -lsnappy\nLibs.private: -lstdc++\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )
}
