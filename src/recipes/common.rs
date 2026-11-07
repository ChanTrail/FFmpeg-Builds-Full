//! 通用配方目录锚：每库一个模块，全部平台共享（单库多平台条目也在
//! 此——glib 有 mac/windows 两条表项）。
//! Common recipe dir anchor: one module per library, shared by every
//! platform (multi-platform entries of one lib live here too — glib has
//! both a mac and a windows table entry).

pub mod aom;
pub mod aribb24;
pub mod aribcaption;
pub mod avisynth;
pub mod brotli;
pub mod bs2b;
pub mod bzip2;
pub mod caca;
pub mod cairo;
pub mod codec2;
pub mod davs2;
pub mod flite;
pub mod fontconfig;
pub mod freetype;
pub mod fribidi;
pub mod glib;
pub mod gmp;
pub mod gnutls;
pub mod gsm;
pub mod harfbuzz;
pub mod ilbc;
pub mod kvazaar;
pub mod lc3;
pub mod lcevcdec;
pub mod lensfun;
pub mod leptonica;
pub mod libcdio;
pub mod libcdio_paranoia;
pub mod libcurl;
pub mod libdvdcss;
pub mod libdvdnav;
pub mod libdvdread;
pub mod libffi;
pub mod libpng;
pub mod librist;
pub mod librsvg;
pub mod librtmp;
pub mod libxml2;
pub mod modplug;
pub mod mysofa;
pub mod nettle;
pub mod openapv;
pub mod pango;
pub mod pcre2;
pub mod pixman;
pub mod pocketsphinx;
pub mod qrencode;
pub mod quirc;
pub mod rabbitmq;
pub mod shine;
pub mod sndfile;
pub mod speex;
pub mod svtjpegxs;
pub mod tesseract;
pub mod uavs3d;
pub mod vo_amrwbenc;
pub mod vvenc;
pub mod whisper;
pub mod xavs2;
pub mod xevd;
pub mod xeve;
pub mod zlib;
pub mod zstd;

use crate::extras::Recipe;

/// 本目录全部表项 / every entry in this dir
pub fn recipes() -> Vec<Recipe> {
    let mut v = vec![
        aom::recipe(),
        aribb24::recipe(),
        aribcaption::recipe(),
        avisynth::recipe(),
        brotli::recipe(),
        bs2b::recipe(),
        bzip2::recipe(),
        caca::recipe(),
        cairo::recipe(),
        codec2::recipe(),
        davs2::recipe(),
        flite::recipe(),
        fontconfig::recipe(),
        freetype::recipe(),
        fribidi::recipe(),
        gmp::recipe(),
        gnutls::recipe(),
        gsm::recipe(),
        harfbuzz::recipe(),
        ilbc::recipe(),
        kvazaar::recipe(),
        lc3::recipe(),
        lcevcdec::recipe(),
        lensfun::recipe(),
        leptonica::recipe(),
        libcdio::recipe(),
        libcdio_paranoia::recipe(),
        libcurl::recipe(),
        libdvdcss::recipe(),
        libdvdnav::recipe(),
        libdvdread::recipe(),
        libffi::recipe(),
        libpng::recipe(),
        librist::recipe(),
        librsvg::recipe(),
        librtmp::recipe(),
        libxml2::recipe(),
        modplug::recipe(),
        mysofa::recipe(),
        nettle::recipe(),
        openapv::recipe(),
        pango::recipe(),
        pcre2::recipe(),
        pixman::recipe(),
        pocketsphinx::recipe(),
        qrencode::recipe(),
        quirc::recipe(),
        rabbitmq::recipe(),
        shine::recipe(),
        sndfile::recipe(),
        speex::recipe(),
        svtjpegxs::recipe(),
        tesseract::recipe(),
        uavs3d::recipe(),
        vo_amrwbenc::recipe(),
        vvenc::recipe(),
        whisper::recipe(),
        xavs2::recipe(),
        xevd::recipe(),
        xeve::recipe(),
        zlib::recipe(),
        zstd::recipe(),
    ];
    // glib 是双表项模块（mac+windows）/ glib is the two-entry module
    v.extend(glib::recipes());
    v
}
