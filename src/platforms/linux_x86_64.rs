//! Linux x86_64 建模块入口（gcc 工具链，实现共享于 linux_common）。
//! Linux x86_64 build-module entry (gcc toolchain; implementation shared
//! in linux_common).

use crate::platforms::{self, Pipeline};
use crate::sys::Arch;

/// 本平台流水线 / this platform's pipeline
pub fn pipeline() -> Pipeline {
    Pipeline {
        init_env: || platforms::linux_common::init_env(Arch::X86_64),
        build,
        packages: platforms::linux_common::packages,
        extras: platforms::linux_common::extras_stage,
        ffmpeg: platforms::linux_common::ffmpeg,
        package: platforms::linux_common::package,
        save_deps: platforms::linux_common::save,
    }
}

/// 全流水线：packages → extras(恢复+配方) → ffmpeg → package
/// The whole pipeline: packages -> extras (restore+recipes) -> ffmpeg ->
/// package
fn build() -> Result<(), String> {
    platforms::linux_common::packages()?;
    platforms::linux_common::extras_stage()?;
    platforms::linux_common::ffmpeg()?;
    platforms::linux_common::package()
}
