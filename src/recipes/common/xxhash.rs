//! xxhash 配方：高速哈希库（libplacebo 缓存哈希提速件；cmake 工程在
//! cmake_unofficial/ 子目录）
//! xxhash recipe: the fast hash library (libplacebo's cache-hash
//! accelerator; the cmake project lives in the cmake_unofficial/
//! subdir)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // windows 的 libplacebo 走 pacman 的 xxhash，本配方服务 linux/mac /
    // windows libplacebo uses pacman's xxhash — this recipe serves
    // linux/mac
    let mut x = r("xxhash", "linux,mac", "libxxhash", "-", body);
    x.src = include_str!("xxhash.rs");
    x
}

/// 构建 xxhash / Build xxhash
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // cmake 工程在 cmake_unofficial/ 子目录；XXHASH_BUILD_XXHSUM=OFF
    // 跳过 CLI 工具 / the cmake project lives in cmake_unofficial/;
    // XXHASH_BUILD_XXHSUM=OFF skips the CLI tool
    buildsys::cmake_build(
        &src.join("cmake_unofficial"),
        &[
            "-DBUILD_SHARED_LIBS=OFF",
            "-DXXHASH_BUILD_XXHSUM=OFF",
            "-DXXHASH_BUILD_ENABLE_INLINE_API=OFF",
        ],
    )
}
