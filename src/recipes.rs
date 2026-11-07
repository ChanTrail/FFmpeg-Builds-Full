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
    ("gmp", ""),
    ("nettle", ""),
    ("gnutls", ""),
    ("speex", ""),
    ("shine", ""),
    ("codec2", ""),
    ("ilbc", ""),
    ("gsm", ""),
    ("lc3", ""),
    ("sndfile", ""),
    ("bs2b", ""),
    ("modplug", ""),
    ("vo-amrwbenc", ""),
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
    ("cairo", ""),
    ("freetype", ""),
    ("fribidi", ""),
    ("harfbuzz", ""),
    // libxml2 在 fontconfig 前：fontconfig 的 -Dxml-backend=libxml2 要
    // 它的 .pc / libxml2 before fontconfig — its -Dxml-backend=libxml2
    // resolves our libxml-2.0.pc
    ("libxml2", ""),
    ("fontconfig", ""),
    ("pango", ""),
    ("librsvg", ""),
    ("svtjpegxs", ""),
    ("openapv", ""),
    ("aom", ""),
    ("xevd", ""),
    ("xeve", ""),
    ("libcdio", ""),
    ("libcdio-paranoia", ""),
    ("caca", ""),
    ("rabbitmq", ""),
    ("librist", ""),
    ("libcurl", ""),
    ("librtmp", ""),
    ("libdvdcss", ""),
    ("libdvdread", ""),
    ("libdvdnav", ""),
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
