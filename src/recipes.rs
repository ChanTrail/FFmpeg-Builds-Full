//! 配方注册表：66 个 extras 表项——每库一个模块，按适用平台分目录
//! （common/ 共享、linux/ 与 windows/ 平台专属）。
//! Recipe registry: the 66 extras entries — one module per library, split
//! by applicable platform (common/ shared, linux/ and windows/
//! platform-only).
//! 每库文件自带 `recipe()`（或 glib 的 `recipes()`，双表项），并用
//! include_str! 把自身源码填进 src 字段——配方源一变 .ok 哈希就变，
//! 缓存自动失效。
//! Each lib file carries `recipe()` (or `recipes()` for glib's two
//! entries) and fills its own src field via include_str! — any recipe
//! edit changes the .ok hash and invalidates the cache.

mod common;
mod linux;
mod windows;

use crate::extras::Recipe;

/// 组表构造器 / group-entry constructor
pub const fn r(
    name: &'static str,
    targets: &'static str,
    check: &'static str,
    flags: &'static str,
    run: fn(&crate::extras::Ctx) -> Result<(), String>,
) -> Recipe {
    Recipe {
        name,
        targets,
        check,
        flags,
        run,
        src: "",
    }
}

/// 权威表序（第二列是 targets 必须包含的子串，只为同名条目消歧——
/// glib 在 mac/windows 各占一行）
/// Canonical table order (second element is a substring the entry's
/// targets must contain —
/// disambiguation only, e.g. glib appears once per OS)
const ORDER: &[(&str, &str)] = &[
    ("bzip2", ""),
    ("zstd", ""),
    ("brotli", ""),
    // 早期基础库：xz(base lzma)、ogg(vorbis/theora 前置)、snappy、
    // mbedtls(libssh crypto 后端) / early base libs: xz (base lzma),
    // ogg (vorbis/theora prereq), snappy, mbedtls (libssh crypto)
    ("xz", ""),
    ("ogg", ""),
    ("libsnappy", ""),
    ("mbedtls", ""),
    ("gmp", ""),
    ("nettle", ""),
    ("gnutls", ""),
    ("speex", ""),
    ("shine", ""),
    ("codec2", ""),
    ("ilbc", ""),
    ("gsm", ""),
    ("lc3", ""),
    // vorbis/opus 在 sndfile 前：sndfile 的 ogg 封装支持顺带接上 /
    // vorbis/opus before sndfile so its ogg-mux support hooks up
    ("vorbis", ""),
    ("opus", ""),
    ("sndfile", ""),
    ("bs2b", ""),
    ("modplug", ""),
    ("vo-amrwbenc", ""),
    // 音频编解码簇 / audio codec cluster
    // lame 的 decoder 半边要 libmpg123——必须在 lame 前 /
    // lame's decoder half needs libmpg123 — must precede lame
    ("mpg123", ""),
    ("lame", ""),
    ("twolame", ""),
    ("opencore-amr", ""),
    ("soxr", ""),
    ("rubberband", ""),
    ("openmpt", ""),
    ("gme", ""),
    ("chromaprint", ""),
    ("openal", ""),
    ("mysofa", ""),
    ("flite", ""),
    ("pocketsphinx", ""),
    ("qrencode", ""),
    ("quirc", ""),
    ("pcre2", ""),
    ("libffi", ""),
    ("glib", "mac"),
    ("glib", "windows"),
    ("lensfun", ""),
    ("zlib", ""),
    ("libpng", ""),
    ("leptonica", ""),
    ("tesseract", ""),
    ("pixman", ""),
    ("lcms2", ""),
    ("cairo", ""),
    ("freetype", ""),
    ("fribidi", ""),
    ("harfbuzz", ""),
    // libxml2 在 fontconfig 前：fontconfig 的 -Dxml-backend=libxml2 要
    // 它的 .pc / libxml2 before fontconfig — its -Dxml-backend=libxml2
    // resolves our libxml-2.0.pc
    ("libxml2", ""),
    ("fontconfig", ""),
    // libass 吃 freetype/fribidi/harfbuzz/fontconfig 全套 /
    // libass consumes the whole freetype/fribidi/harfbuzz/fontconfig set
    ("libass", ""),
    ("pango", ""),
    ("librsvg", ""),
    ("svtjpegxs", ""),
    ("openapv", ""),
    ("aom", ""),
    ("xevd", ""),
    ("xeve", ""),
    // 视频编解码 + 图像/滤镜簇（theora 要 ogg、libjxl 自带
    // hwy/skcms/brotli 子模块）/ video codec + image/filter cluster
    // (theora wants ogg; libjxl brings bundled hwy/skcms/brotli
    // submodules)
    ("x264", ""),
    ("x265", ""),
    ("dav1d", ""),
    ("libvpx", ""),
    ("svtav1", ""),
    ("rav1e", ""),
    ("openh264", ""),
    ("xvid", ""),
    ("theora", ""),
    ("openjpeg", ""),
    ("libjxl", ""),
    ("libwebp", ""),
    ("zimg", ""),
    ("vidstab", ""),
    ("vmaf", ""),
    ("frei0r", ""),
    ("zvbi", ""),
    ("libcdio", ""),
    ("libcdio-paranoia", ""),
    ("caca", ""),
    ("rabbitmq", ""),
    ("librist", ""),
    ("libcurl", ""),
    ("librtmp", ""),
    // 协议簇：libssh 要 mbedtls、libsrt 要 gnutls（都在前）/
    // protocol cluster: libssh wants mbedtls, libsrt wants gnutls
    // (both earlier)
    ("libssh", ""),
    ("libsrt", ""),
    ("libzmq", ""),
    ("libdvdcss", ""),
    ("libdvdread", ""),
    ("libdvdnav", ""),
    ("libbluray", ""),
    ("aribb24", ""),
    ("aribcaption", ""),
    ("kvazaar", ""),
    ("alsa", ""),
    ("vdpau", ""),
    ("lz4", ""),
    ("raw1394", ""),
    ("dc1394", ""),
    ("vulkan", ""),
    ("opencl", ""),
    ("whisper", ""),
    ("libplacebo", ""),
    ("lilv", ""),
    ("lcevcdec", ""),
    ("vvenc", ""),
    ("uavs3d", ""),
    ("davs2", ""),
    ("xavs2", ""),
    ("avisynth", ""),
    // 尾簇：sdl2/vapoursynth 消费面广放最后；ffnvcodec/amf 纯头文件
    // 安装，位置随意 / tail cluster: sdl2/vapoursynth last for their
    // broad consumption; ffnvcodec/amf are header-only installs and can
    // sit anywhere
    ("sdl2", ""),
    ("vapoursynth", ""),
    ("ffnvcodec", ""),
    ("amf", ""),
];

/// 组装全表：顺序即依赖序（敏感：whisper 必须在 vulkan 后、libdvdcss
/// 在 libdvdread 前……）
/// Assemble the full table in dependency order (sensitive: whisper after
/// vulkan, libdvdcss before
/// libdvdread, ...)
pub fn table() -> Vec<Recipe> {
    let mut v = Vec::new();
    v.extend(common::recipes());
    v.extend(linux::recipes());
    v.extend(windows::recipes());
    let key = |r: &Recipe| -> usize {
        ORDER
            .iter()
            .position(|(n, t)| *n == r.name && (t.is_empty() || r.targets.contains(t)))
            .unwrap_or(usize::MAX)
    };
    v.sort_by_key(|r| key(r));
    debug_assert_eq!(v.len(), ORDER.len(), "recipe table drifted from ORDER");
    for r in &v {
        debug_assert_ne!(key(r), usize::MAX, "recipe {} missing from ORDER", r.name);
    }
    v
}
