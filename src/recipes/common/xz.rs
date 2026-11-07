//! xz 配方：liblzma（lzma/xz 压缩，base.yaml 恒启，linux 的 xz-utils 只给二进制不给开发库）
//! xz recipe: liblzma (lzma compression — a base.yaml always-on lib; linux's xz-utils ships the binaries but no dev lib)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("xz", "linux,mac", "liblzma", "-", body);
    x.src = include_str!("xz.rs");
    x
}

/// 构建 xz/liblzma / Build xz (liblzma only)
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 只要库——xz/xzdec/lzmainfo 工具和脚本一概不装 /
    // library only — the xz/xzdec/lzmainfo tools and scripts stay out
    buildsys::autotools_build(
        &src,
        &[
            "--disable-xz",
            "--disable-xzdec",
            "--disable-lzmadec",
            "--disable-lzmainfo",
            "--disable-lzma-links",
            "--disable-scripts",
            "--disable-doc",
            "--disable-nls",
        ],
    )
}
