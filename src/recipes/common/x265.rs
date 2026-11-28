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
    // dynamicHDR10 的 vendored json11 裸用 uint8_t 却不 include
    // <cstdint>——老工具链经传递包含蒙混过关，gcc15（ubuntu-26.04）
    // 的头卫生直接判死（实证：linux-arm64 非 legacy 腿 json11.cpp
    // 成片 'uint8_t does not name a type'；此文件只在
    // ENABLE_HDR10_PLUS=ON 时才编译，所以雷是本轮新踩出来的）。
    // 幂等补丁：已有 cstdint 就跳过 / the vendored json11 inside
    // dynamicHDR10 uses uint8_t without including <cstdint> — older
    // toolchains smuggled it through transitive includes, but gcc15's
    // (ubuntu-26.04) header hygiene is fatal (empirical: the
    // linux-arm64 non-legacy leg fills with 'uint8_t does not name a
    // type'; the file only compiles under ENABLE_HDR10_PLUS=ON, which
    // is why this landmine only now went off). Idempotent patch: skip
    // when cstdint is already there
    let j11 = src.join("source/dynamicHDR10/json11/json11.cpp");
    if j11.is_file() {
        let c = crate::fsx::read(&j11);
        if !c.contains("#include <cstdint>") && c.contains("#include <cassert>") {
            crate::fsx::write(
                &j11,
                &c.replace(
                    "#include <cassert>",
                    "#include <cassert>\n#include <cstdint>",
                ),
            )?;
        }
    }
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
