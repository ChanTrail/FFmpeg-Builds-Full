//! lame 配方：MP3 编码器
//! lame recipe: the MP3 encoder

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("lame", "linux,mac", "lame", "--enable-libmp3lame", body);
    x.src = include_str!("lame.rs");
    x
}

/// 构建 libmp3lame / Build libmp3lame
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = crate::sys::env();
    let src = ctx.src()?;
    // --disable-frontend：只出 libmp3lame，不编 lame 命令行 /
    // --disable-frontend: libmp3lame only, no lame CLI
    buildsys::autotools_build(&src, &["--disable-frontend"])?;
    // 上游不随包发 lame.pc——pkg-config 探针全靠手写的这个 /
    // upstream ships no lame.pc — the pkg-config probe relies on this
    // hand-written one
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/lame.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: lame\nDescription: MPEG Audio Layer III encoder\nVersion: 3.100\nLibs: -L${{libdir}} -lmp3lame\nLibs.private: -lm\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )
}
