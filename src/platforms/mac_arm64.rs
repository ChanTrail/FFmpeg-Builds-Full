//! macOS arm64 建模块入口（clang 工具链，实现在 mac_common）。
//! macOS arm64 build-module entry (clang toolchain; implementation in
//! mac_common).

use crate::platforms::{self, Pipeline};
use crate::sys::Arch;

/// 本平台流水线 / this platform's pipeline
pub fn pipeline() -> Pipeline {
    Pipeline {
        init_env: || platforms::mac_common::init_env(Arch::Arm64),
        build,
        packages: platforms::mac_common::packages,
        extras: platforms::mac_common::extras_stage,
        ffmpeg: platforms::mac_common::ffmpeg,
        ffmpeg_plan: platforms::mac_common::ffmpeg_plan,
        package: platforms::mac_common::package,
        save_deps: platforms::mac_common::save,
    }
}

/// 全流水线：packages → extras(恢复+配方) → ffmpeg → package
/// The whole pipeline: packages -> extras (restore+recipes) -> ffmpeg ->
/// package
fn build() -> Result<(), String> {
    platforms::mac_common::packages()?;
    platforms::mac_common::extras_stage()?;
    platforms::mac_common::ffmpeg()?;
    platforms::mac_common::package()
}
