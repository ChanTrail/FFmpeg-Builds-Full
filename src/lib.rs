//! FFmpeg-Builds-Full 构建驱动库：全部功能模块的家——入口二进制
//! （src/main.rs 的 FFmpeg-Builds-Full 与 src/bin/ 下的独立工具）都是
//! 本库的薄壳，逻辑只在库内存在一份。
//! The FFmpeg-Builds-Full build-driver library: home of every functional
//! module — the entry binaries (FFmpeg-Builds-Full in src/main.rs and
//! the standalone tools under src/bin/) are thin shells over this library,
//! so each piece of logic exists exactly once.
//! 模块布局 / module layout:
//!   core/       基础设施：进程/环境、日志、文件系统、网络、JSON、
//!               构建系统、pkg-config / infrastructure
//!   manifest/   features/、sources/、packages/ 清单加载解析 / YAML manifests
//!   engine/     extras 配方引擎 / the extras recipe engine
//!   platforms/  平台×架构构建模块 / per platform×arch build modules
//!   recipes/    配方注册表 / the recipe registry
//!   stages/     流水线阶段：deps 缓存、FFmpeg 本体、冒烟 / pipeline stages

pub mod core;
pub mod engine;
pub mod manifest;
pub mod platforms;
pub mod recipes;
pub mod stages;

// 根级重导出：crate::sys / crate::fsx 等扁平路径对库内模块与外部
// 二进制一致有效，调用点零改动 / root re-exports: the flat crate::sys /
// crate::fsx paths work alike for in-library modules and external
// binaries — zero changes at call sites
pub use core::{ar, buildsys, coff, fsx, json, log, net, pkgcfg, sys};
pub use engine::extras;
pub use manifest::{features, patches, sources};
pub use stages::{deps, ffmpeg, smoke};
