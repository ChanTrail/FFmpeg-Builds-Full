//! 平台构建模块分发：每个 <os>_<arch> 一个独立模块，接口形状一致。
//! Platform build-module dispatch: one independent module per <os>_<arch>,
//! all exposing the same Pipeline shape.
//! 各模块内部可再分 os 级共享助手（如 platforms/windows_common.rs），
//! 但入口永远按平台×架构独立——平台差异只进对应文件。
//! A module may share OS-level helpers internally (e.g.
//! platforms/windows_common.rs), but the entry module is always per
//! platform×arch — platform differences live only in their own files.

pub mod linux_arm64;
pub mod linux_common;
pub mod linux_x86_64;
pub mod mac_arm64;
pub mod mac_common;
pub mod windows_arm64;
pub mod windows_common;
pub mod windows_deps;
pub mod windows_x86_64;

use std::path::PathBuf;

/// 平台流水线：各阶段函数指针 / the per-platform pipeline stages
pub struct Pipeline {
    /// 探测并安装全局环境（sys::set_env）/ probe and install the env
    pub init_env: fn() -> Result<(), String>,
    /// 全流水线 / the whole pipeline
    pub build: fn() -> Result<(), String>,
    /// 安装/确认平台包栈 / install or verify the package stack
    pub packages: fn() -> Result<(), String>,
    /// 跑 extras 引擎 / run the extras engine
    pub extras: fn() -> Result<(), String>,
    /// 取源码+configure+编译 FFmpeg / fetch+configure+build FFmpeg
    pub ffmpeg: fn() -> Result<(), String>,
    /// 打包并校验产物 / package and verify the artifact
    pub package: fn() -> Result<(), String>,
    /// 保存依赖缓存 / save the deps cache
    pub save_deps: fn() -> Result<(), String>,
}

/// 目标平台探测：宿主机即目标机（交叉编译暂不支持）
/// Target detection: the host is the target (no cross-compile yet)
pub fn detect() -> Option<fn() -> Pipeline> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some(windows_x86_64::pipeline),
        ("windows", "aarch64") => Some(windows_arm64::pipeline),
        ("linux", "x86_64") => Some(linux_x86_64::pipeline),
        ("linux", "aarch64") => Some(linux_arm64::pipeline),
        ("macos", "aarch64") => Some(mac_arm64::pipeline),
        _ => None,
    }
}

/// 平台目录名 <os>-<arch>（features/sources 清单键）
/// Platform dir name <os>-<arch> (the features/sources manifest key)
#[allow(dead_code)]
pub fn plat_key() -> String {
    crate::sys::env().plat()
}

/// 工作目录根 / the work-dir root
#[allow(dead_code)]
pub fn work_root() -> PathBuf {
    std::env::var("FFBUILD_WORK")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            crate::sys::env()
                .repo_root
                .join(format!("ffmpeg-{}", crate::sys::env().plat()))
        })
}
