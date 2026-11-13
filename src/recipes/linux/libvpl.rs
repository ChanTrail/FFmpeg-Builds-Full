//! libvpl 配方：Intel oneVPL（QSV）运行时静态派发器（x86-only）
//! libvpl recipe: Intel oneVPL (QSV) runtime static dispatcher (x86 only)
//!
//! sources/linux-x86_64.yaml 早已声明 git 源但一直没人消费——features 探针
//! file:lib/libvpl.a 只认前缀内归档，apt 的 libvpl-dev 救不了它，只能本
//! 地建。Windows 的同名项走 pacman（packages/windows.yaml）。
//! sources/linux-x86_64.yaml declared the git source long ago but nobody
//! consumed it — the feature probe file:lib/libvpl.a only accepts an
//! archive inside the prefix, apt's libvpl-dev can't satisfy it, so a
//! recipe is the only route. Windows' entry of the same name rides pacman
//! (packages/windows.yaml).

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libvpl",
        "linux:x86_64",
        "vpl,file:lib/libvpl.a",
        "--enable-libvpl",
        body,
    );
    x.src = include_str!("libvpl.rs");
    x
}

/// 构建 libvpl 静态派发器 / Build the static libvpl dispatcher
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::cmake_build(
        &src,
        &[
            // 源码落在哪个 tag 不固定（git 浮动源 + CI/tag 回退），
            // 新老两套变量名一起给，未识别的 -D 只是告警：
            // which tag the git-floating source lands on varies (CI/tag
            // fallback), so pass both generations of variable names —
            // unrecognized -D options only warn:
            // v2023.x 时代 BUILD_TOOLS=ON 会编 tools/legacy 旧示例代码，
            // 撞新 GCC（sample_vpp_frc_adv.cpp 缺 <cstdint>）——必须关；
            // 新 master 已无该变量 / the v2023.x-era BUILD_TOOLS=ON builds
            // tools/legacy sample code that breaks on new GCC
            // (sample_vpp_frc_adv.cpp missing <cstdint>) — must be off;
            // newer master no longer has the variable
            "-DBUILD_TOOLS=OFF",
            "-DBUILD_EXAMPLES=OFF",
            // 例子源码也别装进前缀：INSTALL_EXAMPLES 是新名，
            // INSTALL_EXAMPLE_CODE 是 v2023.x 旧名 / keep example sources
            // out of the prefix: INSTALL_EXAMPLES is the new name,
            // INSTALL_EXAMPLE_CODE the v2023.x one
            "-DINSTALL_EXAMPLES=OFF",
            "-DINSTALL_EXAMPLE_CODE=OFF",
        ],
    )
}
