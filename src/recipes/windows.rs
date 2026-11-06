//! Windows 专属配方目录锚：仅本平台需要的库（头文件/加载器 shim、
//! libplacebo、LV2 栈）。
//! Windows-only recipe dir anchor: libraries this platform alone needs
//! (header/loader shims, libplacebo, the LV2 stack).

pub mod libplacebo;
pub mod lilv;
pub mod opencl;
pub mod vulkan;

use crate::extras::Recipe;

/// 本目录全部表项 / every entry in this dir
pub fn recipes() -> Vec<Recipe> {
    vec![
        libplacebo::recipe(),
        lilv::recipe(),
        opencl::recipe(),
        vulkan::recipe(),
    ]
}
