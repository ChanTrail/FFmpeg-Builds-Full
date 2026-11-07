//! xvid 配方：Xvid MPEG-4 视频（autotools 根在 build/generic 子目录）
//! xvid recipe: Xvid MPEG-4 (its autotools root lives in build/generic)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("xvid", "linux,mac", "xvidcore", "--enable-libxvid", body);
    x.src = include_str!("xvid.rs");
    x
}

/// 构建 xvidcore / Build xvidcore
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = crate::sys::env();
    // autotools 工程嵌在 build/generic——不是仓库根 /
    // the autotools project is nested under build/generic, not the root
    buildsys::autotools_build(&ctx.src()?.join("build/generic"), &[])?;
    // 上游不随包发 xvidcore.pc——pkg-config 探针全靠手写的这个 /
    // upstream ships no xvidcore.pc — the pkg-config probe relies on
    // this hand-written one
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/xvidcore.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: xvidcore\nDescription: Xvid MPEG-4 video codec\nVersion: 1.3.7\nLibs: -L${{libdir}} -lxvidcore\nLibs.private: -lm\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )
}
