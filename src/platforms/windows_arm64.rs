//! Windows arm64 建模块入口（CLANGARM64/clang 工具链）。
//! Windows arm64 build-module entry (CLANGARM64 / clang toolchain).

use crate::platforms::{self, Pipeline};
use crate::sys::Arch;

/// 本平台流水线 / this platform's pipeline
pub fn pipeline() -> Pipeline {
    Pipeline {
        init_env: || platforms::windows_common::init_env(Arch::Arm64),
        build,
        packages: platforms::windows_common::packages,
        extras: platforms::windows_deps::extras_stage,
        ffmpeg: platforms::windows_common::ffmpeg,
        package: platforms::windows_common::package,
        save_deps: platforms::windows_deps::save,
    }
}

/// 全流水线：packages → extras(恢复+配方+fixup) → ffmpeg → package
/// The whole pipeline: packages -> extras (restore+recipes+fixup) ->
/// ffmpeg -> package
fn build() -> Result<(), String> {
    platforms::windows_common::packages()?;
    platforms::windows_deps::extras_stage()?;
    platforms::windows_common::ffmpeg()?;
    platforms::windows_common::package()
}
