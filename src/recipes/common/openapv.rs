//! openapv 配方：APV 编解码（Linux 由 BtbN 构建、MSYS2 有包；extras
//! 供 macOS 并兜底）
//! openapv recipe: APV codec (BtbN builds it on Linux, MSYS2 packages it;
//! extras provides macOS and is the fallback)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("openapv", "all", "oapv", "--enable-liboapv", body);
    x.src = include_str!("openapv.rs");
    x
}

/// 构建 OpenAPV / Build OpenAPV
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 不需要命令行工具 / the cli app is not needed
    fsx::write(&src.join("app/CMakeLists.txt"), "")?;
    buildsys::cmake_build(&src, &["-DOAPV_APP_STATIC_BUILD=ON", "-DENABLE_TESTS=OFF"])?;
    // 只删 openapv 自己的产物——绝不能删整个 $PREFIX/bin（workspace 的
    // 工具住在那里）/ remove only openapv's own extras — never wipe
    // $PREFIX/bin (the workspace's own tools live there)
    fsx::rm_glob(&e.prefix_win.join("bin"), "oapv*");
    fsx::rm_rf(&e.prefix_win.join("lib/oapv"));
    fsx::rm_rf(&e.prefix_win.join("lib/import"));
    fsx::rm_glob(&e.prefix_win.join("lib"), "liboapv.so*");
    fsx::rm_glob(&e.prefix_win.join("lib"), "liboapv*.dylib");
    fsx::rm_glob(&e.prefix_win.join("lib"), "liboapv*.dll*");
    // inc/oapv.h 在未定义 OAPV_STATIC_DEFINE 时 #include
    // <oapv/oapv_exports.h>——该文件必须存在，否则消费者编译死在缺头文件
    // inc/oapv.h includes <oapv/oapv_exports.h> unless OAPV_STATIC_DEFINE —
    // the file must exist or every consumer compile dies
    let eh = e.prefix_win.join("include/oapv/oapv_exports.h");
    if !eh.is_file() {
        fsx::write(
            &eh,
            "#ifndef OAPV_EXPORTS_H\n#define OAPV_EXPORTS_H\n\n#define OAPV_EXPORT\n#define OAPV_NO_EXPORT\n\n#endif\n",
        )?;
    }
    // 并入已有键行——裸追加会产生第二个 Libs.private/Cflags.private，
    // pkg-config --static 对重复字段 exit 1 / merge into existing key
    // lines — a raw append emits a second field that pkg-config exits on
    pkgcfg::libs_private("oapv", &["-lm"]);
    pkgcfg::cflags_private("oapv", &["-DOAPV_STATIC_DEFINE"]);
    Ok(())
}
