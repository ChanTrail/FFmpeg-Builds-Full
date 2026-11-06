//! 基础设施锚文件：进程/环境、日志、文件系统、网络、JSON、
//! 构建系统、pkg-config、归档符号——底层模块都归 core/。
//! Infrastructure anchor: process/env, logging, filesystem, network, JSON,
//! build systems, pkg-config and archive symbols — all low-level modules
//! live under core/.

pub mod ar;
pub mod buildsys;
pub mod fsx;
pub mod json;
pub mod log;
pub mod net;
pub mod pkgcfg;
pub mod sys;
