//! libxcb 配方：X 协议 C 绑定——一次产出 xcb 及全部扩展子库
//! libxcb recipe: the X protocol C binding — one build yields xcb plus
//! every extension sublibrary

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // 多探针：xcb-shm/shape/xfixes 子库 .pc 与主库同树产出，全量到场才
    // 算建成——features 的 libxcb-shm/shape/xfixes 特性逐一查它们 /
    // multi-probe: the xcb-shm/shape/xfixes sublibrary .pc files land in
    // the same install as the main one — the build only counts as done
    // when all of them exist (the libxcb-shm/shape/xfixes features probe
    // each one)
    let mut x = r(
        "libxcb",
        "linux",
        "xcb,xcb-shm,xcb-shape,xcb-xfixes",
        "-",
        body,
    );
    x.src = include_str!("libxcb.rs");
    x
}

/// 构建 libxcb 及扩展子库 / Build libxcb plus its extension sublibraries
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // --disable-devel-docs：doxygen 文档；代码生成靠 xcb-proto 装的
    // xcbgen（configure 经 pkg-config 找）/ --disable-devel-docs:
    // doxygen docs; codegen runs off the xcbgen that xcb-proto installed
    // (configure locates it via pkg-config)
    buildsys::autotools_build(&src, &["--disable-devel-docs"])
}
