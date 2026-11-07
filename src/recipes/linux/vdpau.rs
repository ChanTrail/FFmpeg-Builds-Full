//! vdpau 配方：Linux 硬件解码接口
//! vdpau recipe: Linux hardware decode interface

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("vdpau", "linux", "vdpau", "--enable-vdpau", body);
    x.src = include_str!("vdpau.rs");
    x
}

/// 构建 libvdpau 静态库 / Build libvdpau statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // meson.build 硬编码 shared_library()（驱动加载器 ABI）——改成
    // library() 才得到 .a；让 .so 留在前缀会把 -lvdpau 变成 ffmpeg 的
    // NEEDED 条目 / meson.build hardcodes shared_library() — rewrite to
    // library() for a .a; a stray .so turns -lvdpau into a NEEDED entry
    for f in [src.join("src/meson.build"), src.join("meson.build")] {
        if f.is_file() {
            let _ = fsx::sed_i("s/shared_library(/library(/g", &f, &src);
        }
    }
    buildsys::meson_build(
        &src,
        &["-Ddocumentation=false", "-Dmoduledir=/usr/lib64/vdpau"],
    )
}
