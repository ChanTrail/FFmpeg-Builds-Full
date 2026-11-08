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
            // 例子源码别装进前缀（BUILD_SHARED_LIBS=OFF 由 cmake_build
            // 统一注入，BUILD_TESTS 上游默认即 OFF）/ keep example
            // sources out of the prefix (BUILD_SHARED_LIBS=OFF comes from
            // cmake_build; upstream already defaults BUILD_TESTS off)
            "-DINSTALL_EXAMPLES=OFF",
        ],
    )
}
