//! uavs3d 配方：AVS3 解码器
//! uavs3d recipe: AVS3 decoder

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "uavs3d",
        "linux,mac,windows:x86_64",
        "uavs3d",
        "--enable-libuavs3d",
        body,
    );
    x.src = include_str!("uavs3d.rs");
    x
}

/// 构建 uavs3d / Build uavs3d
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let b = src.join("build/linux");
    fsx::install_d(&b)?;
    sys::run_ok(
        "cmake",
        &[
            "-DCMAKE_BUILD_TYPE=Release",
            &format!("-DCMAKE_INSTALL_PREFIX={}", e.prefix),
            "-DCMAKE_INSTALL_LIBDIR=lib",
            "-DCOMPILE_10BIT=1",
            "-DBUILD_SHARED_LIBS=NO",
            "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
            "../..",
        ],
        &b,
    )?;
    // 生成器不保证（MSYS2 cmake 默认 Ninja）——通过 cmake 驱动而不是直
    // 接调 make / the generator is not guaranteed — drive through cmake
    // instead of calling make
    let jobs = e.jobs.to_string();
    sys::run_ok("cmake", &["--build", ".", "-j", &jobs], &b)?;
    sys::run_ok("cmake", &["--build", ".", "--target", "install"], &b)
}
