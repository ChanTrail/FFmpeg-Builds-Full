//! Windows x86_64 建模块入口（UCRT64/gcc 工具链）。
//! Windows x86_64 build-module entry (UCRT64 / gcc toolchain).

use crate::platforms::{self, Pipeline};
use crate::sys::Arch;

/// 本平台流水线 / this platform's pipeline
pub fn pipeline() -> Pipeline {
    Pipeline {
        init_env: || platforms::windows_common::init_env(Arch::X86_64),
        build,
        packages: platforms::windows_common::packages,
        extras: platforms::windows_deps::extras_stage,
        ffmpeg: platforms::windows_common::ffmpeg,
        ffmpeg_plan: platforms::windows_common::ffmpeg_plan,
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
