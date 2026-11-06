//! Linux 专属配方目录锚：仅本平台需要的库（采集设备/硬件接口）。
//! Linux-only recipe dir anchor: libraries this platform alone needs
//! (capture devices / hardware interfaces).

pub mod alsa;
pub mod dc1394;
pub mod lz4;
pub mod raw1394;
pub mod vdpau;

use crate::extras::Recipe;

/// 本目录全部表项 / every entry in this dir
pub fn recipes() -> Vec<Recipe> {
    vec![
        alsa::recipe(),
        dc1394::recipe(),
        lz4::recipe(),
        raw1394::recipe(),
        vdpau::recipe(),
    ]
}
