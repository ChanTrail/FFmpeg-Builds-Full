//! x265 配方：HEVC 编码器（cmake 根在 source/ 子目录）
//! x265 recipe: HEVC encoder (cmake root is the source/ subdir)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("x265", "linux,mac", "x265", "--enable-libx265", body);
    x.src = include_str!("x265.rs");
    x
}

/// 构建 x265 / Build x265
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // cmake 工程在 source/ 子目录 / the cmake project lives in source/
    buildsys::cmake_build(
        &src.join("source"),
        &[
            "-DENABLE_SHARED=OFF",
            "-DENABLE_CLI=OFF",
            "-DHIGH_BIT_DEPTH=ON",
            // HDR10+ 动态元数据支持：实现 vendored 在 x265 树内
            // （source/dynamicHDR10/），ENABLE_HDR10_PLUS 仅是编译
            // 开关，零外部依赖；开启后 x265-param 可用
            // dhdr10-info/dhdr10-opt 注入 HDR10+ SEI / HDR10+
            // dynamic metadata support: the implementation is vendored
            // inside the x265 tree (source/dynamicHDR10/) — the flag is
            // a pure compile switch with zero external deps; once on,
            // the dhdr10-info/dhdr10-opt x265-params can inject HDR10+
            // SEI messages
            "-DENABLE_HDR10_PLUS=ON",
        ],
    )
}
