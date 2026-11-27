//! 配方注册表：67 个 extras 表项——每库一个模块，按适用平台分目录
//! （common/ 共享、linux/ 与 windows/ 平台专属）。
//! Recipe registry: the 67 extras entries — one module per library, split
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
    // iconv 必须最先：它是零依赖叶子库，但 libass/libxml2 等消费者的
    // configure 会探测 iconv——先建 libiconv.a，它们才绑自编实现而非
    // glibc iconv（后者运行时 dlopen gconv，静态二进制里与内嵌 glibc
    // 冲突即崩）/ iconv must come FIRST: it is a zero-dependency leaf,
    // but consumers like libass/libxml2 probe iconv at configure time —
    // building libiconv.a first binds them to our implementation instead
    // of glibc iconv (the latter dlopens gconv at runtime and crashes
    // against the embedded glibc in static binaries)
    ("iconv", ""),
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
    ("fontconfig", "linux,mac"),
    ("fontconfig", "windows"),
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
    ("libvpl", ""),
    ("lz4", ""),
    ("raw1394", ""),
    ("dc1394", ""),
    // linux 平台栈：X11/xcb 链按依赖序（macros→proto→xau/xdmcp→xcb
    // →xtrans→x11→xext→xfixes→xv），libdrm 独立，libva 收尾 / the
    // linux platform stack: the X11/xcb chain in dependency order
    // (macros→proto→xau/xdmcp→xcb→xtrans→x11→xext→xfixes→xv), libdrm
    // standalone, libva last
    ("xorg-macros", ""),
    ("xorgproto", ""),
    ("xcb-proto", ""),
    ("libxau", ""),
    ("libxdmcp", ""),
    ("libxcb", ""),
    ("xtrans", ""),
    ("libx11", ""),
    ("libxext", ""),
    ("libxfixes", ""),
    ("libxv", ""),
    ("libdrm", ""),
    ("libva", ""),
    ("pulseaudio", ""),
    ("vulkan", ""),
    ("opencl", ""),
    ("whisper", ""),
    // HDR 链：libplacebo 的三件依赖（xxhash 缓存哈希、glslang 着色
    // 器编译器、dovi DV 元数据解析）必须先于本体 / the HDR chain:
    // libplacebo's three dependencies (xxhash cache hashing, glslang
    // shader compiler, dovi DV metadata parsing) must precede it
    ("xxhash", ""),
    ("glslang", ""),
    ("dovi", ""),
    // windows/linux 两条同名配方按平台字段分流 / the two same-named
    // windows/linux recipes split on the platform field
    ("libplacebo", "windows"),
    ("libplacebo", "linux"),
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
    // 纯头文件安装，位置随意——ladspa 与 ffnvcodec/amf 同类 /
    // header-only installs can sit anywhere — ladspa is the same
    // class as ffnvcodec/amf
    ("ladspa", ""),
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

#[cfg(test)]
mod tests {
    /// ORDER↔配方表双射校验：debug_assert 只在 CI 真跑 table() 时才
    /// 炸，本地 cargo test 应能提前抓漂移——数量相等、每配方命中唯
    /// 一 ORDER 位、每 ORDER 位被唯一配方命中（两位同名配方错位覆
    /// 盖时数量仍能相等，必须逐位核对）/ ORDER↔recipe bijection
    /// check: the debug_asserts only fire when CI actually runs
    /// table() — a local cargo test should catch drift earlier: counts
    /// equal, every recipe hits a unique ORDER slot and every ORDER
    /// slot is hit by a unique recipe (two same-named recipes
    /// cross-covering the wrong slots keeps the count equal, so the
    /// slots must be verified one by one)
    #[test]
    fn order_bijection() {
        let v = super::table();
        assert_eq!(v.len(), super::ORDER.len(), "recipe/ORDER count drift");
        let key = |r: &super::Recipe| -> Option<usize> {
            super::ORDER
                .iter()
                .position(|(n, t)| *n == r.name && (t.is_empty() || r.targets.contains(t)))
        };
        let mut seen = std::collections::HashSet::new();
        for r in &v {
            let pos = key(r).unwrap_or_else(|| panic!("recipe {} missing from ORDER", r.name));
            assert!(
                seen.insert(pos),
                "recipe {} shares ORDER slot {pos} with another recipe",
                r.name
            );
        }
    }
}
