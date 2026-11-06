//! avisynth 配方：AviSynth+ 头文件（仅头文件——FFmpeg 运行时才加载本
//! 体；旧 80-windows.sh 移植）
//! avisynth recipe: AviSynth+ headers (header-only — FFmpeg loads the
//! body at run time; port of 80-windows.sh)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "avisynth",
        "all",
        "hdrsym:avisynth/avisynth_c.h@avs_get_channel_mask",
        "--enable-avisynth",
        body,
    );
    x.src = include_str!("avisynth.rs");
    x
}

/// 安装 AviSynth+ 头文件 / Install the AviSynth+ headers
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 它的版本检查坚持要求存在 tag / their version check insists on a tag
    let _ = sys::run(
        "git",
        &[
            "-c",
            "user.name=ci",
            "-c",
            "user.email=ci@localhost",
            "tag",
            "-a",
            "ffbuild",
            "-m",
            "ffbuild",
        ],
        &src,
    );
    sys::run_ok(
        "cmake",
        &[
            "-S",
            ".",
            "-B",
            "build",
            "-DCMAKE_BUILD_TYPE=Release",
            &format!("-DCMAKE_INSTALL_PREFIX={}", e.prefix),
            "-DHEADERS_ONLY=ON",
            "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
        ],
        &src,
    )?;
    // `cmake --install` 是 3.15+ 子命令；生成器自己的 install 目标任何
    // 版本都认 / `cmake --install` needs 3.15+; the generator's own
    // install target works on any vintage
    sys::run_ok(
        "cmake",
        &["--build", "build", "--target", "VersionGen"],
        &src,
    )?;
    sys::run_ok("cmake", &["--build", "build", "--target", "install"], &src)
}
