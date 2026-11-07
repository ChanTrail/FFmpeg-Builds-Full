//! 通用配方目录锚：每库一个模块，全部平台共享（单库多平台条目也在
//! 此——glib 有 mac/windows 两条表项）。
//! Common recipe dir anchor: one module per library, shared by every
//! platform (multi-platform entries of one lib live here too — glib has
//! both a mac and a windows table entry).

pub mod amf;
pub mod aom;
pub mod aribb24;
pub mod aribcaption;
pub mod avisynth;
pub mod brotli;
pub mod bs2b;
pub mod bzip2;
pub mod caca;
pub mod cairo;
pub mod chromaprint;
pub mod codec2;
pub mod dav1d;
pub mod davs2;
pub mod ffnvcodec;
pub mod flite;
pub mod fontconfig;
pub mod freetype;
pub mod frei0r;
pub mod fribidi;
pub mod glib;
pub mod gme;
pub mod gmp;
pub mod gnutls;
pub mod gsm;
pub mod harfbuzz;
pub mod ilbc;
pub mod kvazaar;
pub mod lame;
pub mod lc3;
pub mod lcevcdec;
pub mod lcms2;
pub mod lensfun;
pub mod leptonica;
pub mod libass;
pub mod libbluray;
pub mod libcdio;
pub mod libcdio_paranoia;
pub mod libcurl;
pub mod libdvdcss;
pub mod libdvdnav;
pub mod libdvdread;
pub mod libffi;
pub mod libjxl;
pub mod libpng;
pub mod librist;
pub mod librsvg;
pub mod librtmp;
pub mod libsnappy;
pub mod libsrt;
pub mod libssh;
pub mod libvpx;
pub mod libwebp;
pub mod libxml2;
pub mod libzmq;
pub mod lilv;
pub mod mbedtls;
pub mod modplug;
pub mod mpg123;
pub mod mysofa;
pub mod nettle;
pub mod ogg;
pub mod openal;
pub mod openapv;
pub mod opencl;
pub mod opencore_amr;
pub mod openh264;
pub mod openjpeg;
pub mod openmpt;
pub mod opus;
pub mod pango;
pub mod pcre2;
pub mod pixman;
pub mod pocketsphinx;
pub mod qrencode;
pub mod quirc;
pub mod rabbitmq;
pub mod rav1e;
pub mod rubberband;
pub mod sdl2;
pub mod shine;
pub mod sndfile;
pub mod soxr;
pub mod speex;
pub mod svtav1;
pub mod svtjpegxs;
pub mod tesseract;
pub mod theora;
pub mod twolame;
pub mod uavs3d;
pub mod vapoursynth;
pub mod vidstab;
pub mod vmaf;
pub mod vo_amrwbenc;
pub mod vorbis;
pub mod vulkan;
pub mod vvenc;
pub mod whisper;
pub mod x264;
pub mod x265;
pub mod xavs2;
pub mod xevd;
pub mod xeve;
pub mod xvid;
pub mod xz;
pub mod zimg;
pub mod zlib;
pub mod zstd;
pub mod zvbi;

use crate::extras::Recipe;

/// 本目录全部表项 / every entry in this dir
pub fn recipes() -> Vec<Recipe> {
    let mut v = vec![
        amf::recipe(),
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
        chromaprint::recipe(),
        dav1d::recipe(),
        davs2::recipe(),
        ffnvcodec::recipe(),
        flite::recipe(),
        fontconfig::recipe(),
        freetype::recipe(),
        frei0r::recipe(),
        fribidi::recipe(),
        gme::recipe(),
        gmp::recipe(),
        gnutls::recipe(),
        gsm::recipe(),
        harfbuzz::recipe(),
        ilbc::recipe(),
        kvazaar::recipe(),
        lame::recipe(),
        mpg123::recipe(),
        lc3::recipe(),
        lcevcdec::recipe(),
        lcms2::recipe(),
        lensfun::recipe(),
        leptonica::recipe(),
        libass::recipe(),
        libbluray::recipe(),
        libcdio::recipe(),
        libcdio_paranoia::recipe(),
        libcurl::recipe(),
        libdvdcss::recipe(),
        libdvdnav::recipe(),
        libdvdread::recipe(),
        libffi::recipe(),
        libjxl::recipe(),
        libpng::recipe(),
        librist::recipe(),
        librsvg::recipe(),
        librtmp::recipe(),
        libsnappy::recipe(),
        libssh::recipe(),
        libsrt::recipe(),
        libvpx::recipe(),
        libwebp::recipe(),
        libxml2::recipe(),
        libzmq::recipe(),
        lilv::recipe(),
        mbedtls::recipe(),
        modplug::recipe(),
        mysofa::recipe(),
        nettle::recipe(),
        ogg::recipe(),
        openal::recipe(),
        opencore_amr::recipe(),
        openapv::recipe(),
        opencl::recipe(),
        openh264::recipe(),
        openjpeg::recipe(),
        openmpt::recipe(),
        opus::recipe(),
        pango::recipe(),
        pcre2::recipe(),
        pixman::recipe(),
        pocketsphinx::recipe(),
        qrencode::recipe(),
        quirc::recipe(),
        rabbitmq::recipe(),
        rav1e::recipe(),
        rubberband::recipe(),
        sdl2::recipe(),
        shine::recipe(),
        sndfile::recipe(),
        soxr::recipe(),
        speex::recipe(),
        svtav1::recipe(),
        svtjpegxs::recipe(),
        tesseract::recipe(),
        theora::recipe(),
        twolame::recipe(),
        uavs3d::recipe(),
        vapoursynth::recipe(),
        vidstab::recipe(),
        vmaf::recipe(),
        vo_amrwbenc::recipe(),
        vorbis::recipe(),
        vulkan::recipe(),
        vvenc::recipe(),
        whisper::recipe(),
        x264::recipe(),
        x265::recipe(),
        xavs2::recipe(),
        xevd::recipe(),
        xeve::recipe(),
        xvid::recipe(),
        xz::recipe(),
        zimg::recipe(),
        zlib::recipe(),
        zstd::recipe(),
        zvbi::recipe(),
    ];
    // glib 是双表项模块（mac+windows）/ glib is the two-entry module
    v.extend(glib::recipes());
    v
}
