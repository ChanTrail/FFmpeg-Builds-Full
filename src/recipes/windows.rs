//! Windows 专属配方目录锚：仅本平台需要的库（SPIRV-Cross 链的
//! libplacebo——vulkan/opencl/LV2 栈已挪 common 跨平台共享）。
//! Windows-only recipe dir anchor: libraries this platform alone needs
//! (libplacebo on the SPIRV-Cross chain — the vulkan/opencl/LV2 stacks
//! moved to common for cross-platform sharing).

pub mod libplacebo;

use crate::extras::Recipe;

/// 本目录全部表项 / every entry in this dir
pub fn recipes() -> Vec<Recipe> {
    vec![libplacebo::recipe()]
}
