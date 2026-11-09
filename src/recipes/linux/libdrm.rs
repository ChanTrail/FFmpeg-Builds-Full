//! libdrm 配方：DRM 内核接口用户态层（vaapi_drm 路径依赖）
//! libdrm recipe: the DRM kernel interface userland layer (the
//! vaapi_drm path needs it)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libdrm", "linux", "libdrm", "-", body);
    x.src = include_str!("libdrm.rs");
    x
}

/// 构建 libdrm 静态库 / Build libdrm statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // FFmpeg 只要核心 libdrm.pc（drmGetDevice/drmGetVersion 都在
    // libdrm.so 里）；各厂商后端（intel/amdgpu/nouveau/radeon…）要
    // libpciaccess 等额外依赖且无消费者，整排关掉 / FFmpeg only wants
    // the core libdrm.pc (drmGetDevice/drmGetVersion live in libdrm
    // itself); vendor backends (intel/amdgpu/nouveau/radeon…) pull
    // libpciaccess et al. and have no consumer — all disabled
    buildsys::meson_build(
        &src,
        &[
            "-Dintel=disabled",
            "-Damdgpu=disabled",
            "-Dnouveau=disabled",
            "-Dradeon=disabled",
            "-Dvmwgfx=disabled",
            "-Dvc4=disabled",
            "-Domap=disabled",
            "-Dexynos=disabled",
            "-Dfreedreno=disabled",
            "-Dtegra=disabled",
            "-Detnaviv=disabled",
            "-Dman-pages=disabled",
            "-Dtests=false",
            "-Dvalgrind=disabled",
            "-Dinstall-test-programs=false",
            "-Dudev=disabled",
        ],
    )
}
