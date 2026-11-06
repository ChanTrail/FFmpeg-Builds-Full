//! codec2 配方：低码率语音编解码（旧 30-audio.sh 移植）
//! codec2 recipe: low-bitrate speech codec (port of 30-audio.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "codec2",
        "linux,mac,windows",
        "file:lib/libcodec2.a",
        "--enable-libcodec2",
        body,
    );
    x.src = include_str!("codec2.rs");
    x
}

/// 构建 codec2 / Build codec2
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // Windows 上 install() 会跑 CPack 依赖收集脚本，含 tarball 没带的
    // cmake/GetPrerequisites.cmake / on Windows install() runs a CPack
    // dep-gathering script that includes a file the tarball does not ship
    fsx::sed_i(
        "/install(SCRIPT.*GetDependencies/d",
        &src.join("CMakeLists.txt"),
        &src,
    )?;
    // codec2 的 lpc.c 和 speex 的 lsp.c 都导出 lpc_to_lsp/lsp_to_lpc——
    // 内部辅助函数，源码层加命名空间 / both export lpc_to_lsp/lsp_to_lpc —
    // internal helpers, namespace them in source
    for f in fsx::grep_l(&src, &["lpc_to_lsp", "lsp_to_lpc"]) {
        fsx::sed_i(
            "s/lpc_to_lsp/codec2_lpc_to_lsp/g; s/lsp_to_lpc/codec2_lsp_to_lpc/g",
            &f,
            &src,
        )?;
    }
    buildsys::cmake_build(
        &src,
        &["-DUNITTEST=FALSE", "-DLPCNET=OFF", "-DBUILD_TESTING=OFF"],
    )
}
