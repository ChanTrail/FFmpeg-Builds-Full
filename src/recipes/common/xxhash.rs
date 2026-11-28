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
    // cmake 工程的子目录两布局：v0.8.3 及之前在 cmake_unofficial/，
    // v0.8.4 起上游重构挪进 build/cmake/（cmake_unofficial 整目录消
    // 失——实证 beta 首轮三腿 spawn cmake ENOENT，真身是 cwd 不存在
    // 而非 cmake 缺席）/ the cmake project's subdir comes in two
    // layouts: cmake_unofficial/ up to v0.8.3, build/cmake/ since the
    // v0.8.4 upstream restructuring (cmake_unofficial vanished
    // entirely — empirical: beta round one's xxhash legs all died on
    // spawn cmake ENOENT, whose real meaning was a missing cwd, not a
    // missing cmake)
    let cmakedir = ["cmake_unofficial", "build/cmake"]
        .iter()
        .map(|d| src.join(d))
        .find(|p| p.join("CMakeLists.txt").is_file())
        .ok_or("xxhash: no cmake project at cmake_unofficial/ or build/cmake/")?;
    // XXHASH_BUILD_XXHSUM=OFF 跳过 CLI 工具 / XXHASH_BUILD_XXHSUM=OFF
    // skips the CLI tool
    buildsys::cmake_build(
        &cmakedir,
        &[
            "-DBUILD_SHARED_LIBS=OFF",
            "-DXXHASH_BUILD_XXHSUM=OFF",
            "-DXXHASH_BUILD_ENABLE_INLINE_API=OFF",
        ],
    )
}
