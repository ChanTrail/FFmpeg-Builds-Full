//! Linux 专属配方目录锚：仅本平台需要的库（采集设备/硬件接口）。
//! Linux-only recipe dir anchor: libraries this platform alone needs
//! (capture devices / hardware interfaces).

pub mod alsa;
pub mod dc1394;
pub mod libdrm;
pub mod libva;
pub mod libvpl;
pub mod libx11;
pub mod libxau;
pub mod libxcb;
pub mod libxdmcp;
pub mod libxext;
pub mod libxfixes;
pub mod libxv;
pub mod lz4;
pub mod pulseaudio;
pub mod raw1394;
pub mod vdpau;
pub mod xcb_proto;
pub mod xorg_macros;
pub mod xorgproto;
pub mod xtrans;

use crate::extras::Recipe;

/// 本目录全部表项 / every entry in this dir
pub fn recipes() -> Vec<Recipe> {
    vec![
        alsa::recipe(),
        dc1394::recipe(),
        libdrm::recipe(),
        libva::recipe(),
        libvpl::recipe(),
        libx11::recipe(),
        libxau::recipe(),
        libxcb::recipe(),
        libxdmcp::recipe(),
        libxext::recipe(),
        libxfixes::recipe(),
        libxv::recipe(),
        lz4::recipe(),
        pulseaudio::recipe(),
        raw1394::recipe(),
        vdpau::recipe(),
        xcb_proto::recipe(),
        xorg_macros::recipe(),
        xorgproto::recipe(),
        xtrans::recipe(),
    ]
}
