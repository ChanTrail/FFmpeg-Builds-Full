//! FFmpeg 特性清单加载：features/*.yaml → configure 旗标
//! FFmpeg feature lists: features/*.yaml → configure flags
//! 布局 / layout:
//!   features/base.yaml            [zlib, ...]                 —— 恒 enable
//!   features/common.yaml          {libx264: enable | {all: v, <os>-<arch>: v}}  —— 多平台共享
//!   features/misc.yaml            {libfoo: disable | {all: v, <os>-<arch>: v}}  —— 稀有可选
//!   features/<os>-<arch>.yaml     {metal: enable, ...}        —— 本平台独有
//! 规则 / rules:
//! * 键 = FFmpeg configure 特性名（libx264、gnutls、avisynth……必须已知）
//!   key = FFmpeg configure feature name (must be a known feature)
//! * base 恒 --enable（不再探测，也不允许出现在其他文件里）
//!   base is unconditionally --enable (never probed, forbidden elsewhere)
//! * enable 先做可用性探测，库缺失时 warn 降级为 --disable
//!   "enable" is probed first; a missing library warns + degrades to --disable
//! * 已知特性没在任何文件出现 → --disable（默认全 disable 的严格语义）
//!   every KNOWN feature absent from all files → --disable (strict default)
//! * 同一键出现在两个位置 → 报错（禁止重复键；misc 每个键只解析本
//!   平台相关值）/ a key defined twice → hard error (duplicates
//!   forbidden; misc resolves only this platform's relevant value)

use crate::log;
use crate::pkgcfg;
use crate::sys;
use std::collections::BTreeMap;
use std::path::Path;
use yaml_rust2::{Yaml, YamlLoader};

/// 探测规格：逗号分隔——pkg-config 模块名 | file:<前缀内路径> |
/// hdrsym:<头@符号> | arsym:<归档@符号> | always（永不失败）
/// Probe spec: comma-separated — pkg-config module | file:<path under
/// prefix> | hdrsym:<header@symbol> | arsym:<archive@symbol> | always
pub struct Feature {
    /// configure 特性名 / configure feature name
    pub name: &'static str,
    /// 可用性探测规格 / availability probe spec
    pub probe: &'static str,
}

/// 已知特性表：packages/*.yaml 旗标列与 extras 配方表涉及的全部外部库
/// 与 autodetect 特性。JSON 里出现表外键直接报错——笔误不能静默漏特性。
/// probe="always" 表示该特性随平台头文件天然可用（不探测，enable 即
/// 信任；FFmpeg configure 自己兜底）。
/// Known-feature table: every external-library and autodetect feature that
/// packages/*.yaml flag columns and the extras recipes reference. A JSON
/// key outside this table is a hard error — typos must not silently drop.
/// probe="always" means the feature is guaranteed by platform headers
/// (no probe; FFmpeg's configure is the final arbiter).
pub const FEATURES: &[Feature] = &[
    // ---- 压缩 / compression ----
    f("zlib", "zlib"),
    f("bzlib", "file:lib/libbz2.a"),
    f("lzma", "liblzma"),
    f("iconv", "file:lib/libiconv.a"),
    f("libsnappy", "snappy"),
    // ---- TLS / crypto ----
    f("gnutls", "gnutls"),
    f("gmp", "file:lib/libgmp.a"),
    f("openssl", "openssl"),
    f("mbedtls", "mbedtls"),
    f("gcrypt", "gcrypt"),
    f("schannel", "always"),
    f("securetransport", "always"),
    // ---- 视频编解码 / video codecs ----
    f("libx264", "x264"),
    f("libx265", "x265"),
    f("libaom", "aom"),
    f("libdav1d", "dav1d"),
    f("libvpx", "vpx"),
    f("libsvtav1", "SvtAv1Enc"),
    f("librav1e", "rav1e"),
    // FFmpeg 的 libjxl 链测同时要求 libjxl_threads——单查 libjxl 会让
    // 探针放行、configure 半途中止（实证过的丢包方式）/ FFmpeg's libjxl
    // link check also requires libjxl_threads — probing libjxl alone
    // passes the gate then dies mid-configure (the empirical drop mode)
    f("libjxl", "libjxl,libjxl_threads"),
    f("libwebp", "libwebp"),
    f("libopenjpeg", "libopenjp2"),
    // FFmpeg 的 require 是 -ltheoraenc -ltheoradec 两件套——探针同查防
    // 放行后掉 / FFmpeg's require links both -ltheoraenc and -ltheoradec
    // — probe both so the gate can't pass then drop
    f("libtheora", "theoraenc,theoradec"),
    // msys2 的 xvidcore 包不产 .pc——头文件验收通吃配方与 pacman 两路 /
    // msys2's xvidcore package ships no .pc — a header check covers
    // both the recipe and the pacman paths
    f("libxvid", "hdrsym:xvid.h@xvid_global"),
    f("libkvazaar", "kvazaar"),
    f("libopenh264", "openh264"),
    f("liboapv", "oapv"),
    f(
        "libsvtjpegxs",
        "SvtJpegxs,arsym:lib/libSvtJpegxs.a@svt_jpeg_xs_encoder_init",
    ),
    f("libxevd", "xevd"),
    f("libxeve", "xeve"),
    f("liblcevc-dec", "lcevc_dec"),
    f("libvvenc", "libvvenc"),
    f("libuavs3d", "uavs3d"),
    f("libdavs2", "davs2"),
    f("libxavs2", "xavs2"),
    // ---- 音频编解码 / audio codecs ----
    f("libvorbis", "vorbisenc"),
    f("libopus", "opus"),
    f("libmp3lame", "lame"),
    f("libspeex", "speex"),
    f("libshine", "shine"),
    f("libgsm", "file:lib/libgsm.a"),
    f(
        "liblc3",
        "lc3,hdrsym:lc3.h@lc3_hr_setup_encoder,arsym:lib/liblc3.a@lc3_hr_setup_encoder",
    ),
    f("libbs2b", "libbs2b"),
    f("libmodplug", "libmodplug"),
    f("libopenmpt", "libopenmpt"),
    f("libgme", "libgme"),
    f("libopencore-amrnb", "opencore-amrnb"),
    f("libopencore-amrwb", "opencore-amrwb"),
    f("libvo-amrwbenc", "vo-amrwbenc"),
    f("libtwolame", "twolame"),
    f("libsoxr", "soxr"),
    f("librubberband", "rubberband"),
    f("libmysofa", "libmysofa"),
    f("chromaprint", "libchromaprint"),
    f("openal", "openal"),
    f("libcodec2", "file:lib/libcodec2.a"),
    f("libilbc", "file:lib/libilbc.a"),
    f("libflite", "file:lib/libflite.a"),
    f("pocketsphinx", "pocketsphinx"),
    f("libfdk-aac", "libfdk-aac"),
    // ---- 字幕、文字、图形 / subtitles, text, graphics ----
    f("libass", "libass"),
    f("libfreetype", "freetype2"),
    f("libharfbuzz", "harfbuzz"),
    f("libfribidi", "fribidi"),
    f("libfontconfig", "fontconfig"),
    f("lcms2", "lcms2"),
    // cairo 是独立 FFmpeg 特性：drawvg 滤镜挂在它而非 librsvg 上 /
    // cairo is its own FFmpeg feature: the drawvg filter gates on it,
    // not on librsvg
    f("cairo", "cairo"),
    f("librsvg", "librsvg-2.0"),
    f("sdl2", "sdl2"),
    f("libvidstab", "vidstab"),
    f("libvmaf", "libvmaf"),
    f("frei0r", "file:include/frei0r.h"),
    f("libaribcaption", "libaribcaption"),
    f("libaribb24", "aribb24"),
    f("libzvbi", "zvbi-0.2"),
    f("vapoursynth", "vapoursynth"),
    f("libtesseract", "tesseract"),
    f("libqrencode", "libqrencode"),
    f("libquirc", "file:lib/libquirc.a"),
    f(
        "liblensfun",
        "lensfun,hdrsym:lensfun/lensfun.h@lf_db_create",
    ),
    // ---- 容器、协议、光盘 / containers, protocols, discs ----
    f("libxml2", "libxml-2.0"),
    f("libssh", "libssh"),
    f("libsrt", "srt"),
    f("librist", "librist"),
    f("libzmq", "libzmq"),
    f("librabbitmq", "librabbitmq"),
    f("libbluray", "libbluray"),
    // .pc 名是 dvdnav/dvdread 不是 libdvdnav/libdvdread /
    // the .pc names are dvdnav/dvdread, not libdvdnav/libdvdread
    f("libdvdnav", "dvdnav"),
    f("libdvdread", "dvdread"),
    f("libzimg", "zimg"),
    f("libcdio", "libcdio_paranoia"),
    f("libcaca", "caca"),
    f("librtmp", "librtmp"),
    f("libcurl", "libcurl"),
    f(
        "avisynth",
        "hdrsym:avisynth/avisynth_c.h@avs_get_channel_mask",
    ),
    // ---- 硬件加速、采集、杂项 / hwaccel, capture, misc ----
    f("amf", "file:include/AMF/core/Factory.h"),
    f("ffnvcodec", "ffnvcodec"),
    f("libvpl", "file:lib/libvpl.a"),
    f("vulkan", "vulkan"),
    f("opencl", "OpenCL"),
    f("whisper", "whisper"),
    f("libplacebo", "libplacebo"),
    f("lv2", "lilv-0"),
    f("alsa", "alsa"),
    f("vdpau", "vdpau"),
    // libva 装的是 libva.pc——世上没有 vaapi.pc，"vaapi" 必降级 /
    // libva ships libva.pc — no vaapi.pc exists anywhere, so probing
    // "vaapi" degrades unconditionally
    // vaapi hwaccel 实际要 libva+libva-drm 这一对——同配方同产，探针
    // 照实要求 / the vaapi hwaccel really wants the libva+libva-drm pair
    // — same recipe produces both, the probe asks for what FFmpeg needs
    f("vaapi", "libva,libva-drm"),
    f("libdc1394", "libdc1394-2"),
    f("libdrm", "libdrm"),
    f("libtensorflow", "always"),
    f("libtorch", "always"),
    f("libopencv", "opencv4"),
    // ---- autodetect 族：--disable-autodetect 下必须显式点名 ----
    // autodetect family: must be named explicitly under --disable-autodetect
    f("d3d11va", "always"),
    f("d3d12va", "always"),
    f("dxva2", "always"),
    f("mediafoundation", "always"),
    // FFmpeg 内部名带下划线——--enable-v4l2m2m 会被 unknown-option
    // 丢弃（实证）/ FFmpeg's internal name carries the underscore —
    // --enable-v4l2m2m gets dropped as an unknown option (empirical)
    f("v4l2_m2m", "always"),
    // FFmpeg 的 xlib 检查是 x11+xext+xv 三件套（Xvlib.h 那次实证）——
    // 三件同查，缺一不如直接降级 / FFmpeg's xlib check is the
    // x11+xext+xv trio (empirical: the Xvlib.h failure) — probe all
    // three; missing any one, degrading beats a mid-configure drop
    f("xlib", "x11,xext,xv"),
    f("libxcb", "xcb"),
    f("libxcb-shm", "xcb-shm"),
    f("libxcb-shape", "xcb-shape"),
    f("libxcb-xfixes", "xcb-xfixes"),
    f("libpulse", "libpulse"),
    f("libjack", "jack"),
    f("sndio", "sndio"),
    f("ladspa", "file:include/ladspa.h"),
    f("nvdec", "ffnvcodec"),
    f("nvenc", "ffnvcodec"),
    f("cuvid", "ffnvcodec"),
    f("cuda-llvm", "always"),
    f("cuda-nvcc", "always"),
    f("videotoolbox", "always"),
    f("audiotoolbox", "always"),
    f("avfoundation", "always"),
    f("appkit", "always"),
    f("coreimage", "always"),
    f("metal", "always"),
    // ---- 稀有/可选：misc.json 默认 disable / rare & optional ----
    f("decklink", "always"),
    f("libklvanc", "always"),
    f("libsmbclient", "libsmbclient"),
    f("libmfx", "libmfx"),
    f("libopenvino", "openvino"),
];

/// 表项构造 / table-entry constructor
const fn f(name: &'static str, probe: &'static str) -> Feature {
    Feature { name, probe }
}

/// 解析结果 / resolved feature set
pub struct Resolved {
    /// 全部 --enable/--disable 旗标（FEATURES 表序）
    /// All --enable/--disable flags in FEATURES table order
    pub flags: Vec<String>,
    /// 实际启用的特性名 / effectively enabled names
    pub enabled: Vec<String>,
    /// enable 被降级成 disable 的名字 / enables degraded to disable
    pub degraded: Vec<String>,
}

impl Resolved {
    /// 必填特性必须实际启用——探测降级和显式关闭都算缺席，点名早败，
    /// 别让 configure 静默产出一个缺件的二进制 /
    /// required features must be effectively enabled — a probe-degraded
    /// enable and an explicit disable both count as absent; fail early by
    /// name rather than letting configure silently ship a short binary
    pub fn require(&self, required: &[&str]) -> Result<(), String> {
        let absent: Vec<String> = required
            .iter()
            .filter(|n| !self.enabled.iter().any(|e| e == *n))
            .map(|n| {
                if self.degraded.iter().any(|d| d == n) {
                    format!("{n} (probe failed)")
                } else {
                    format!("{n} (not enabled)")
                }
            })
            .collect();
        if absent.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "required libraries not effectively enabled: {}",
                absent.join(", ")
            ))
        }
    }
}

/// 读 YAML 文件为单个文档 / load a YAML file as a single document
fn load_yaml(p: &Path) -> Result<Yaml, String> {
    let text = crate::fsx::read(p);
    let mut docs = YamlLoader::load_from_str(&text)
        .map_err(|e| format!("{}: YAML parse error: {e}", p.display()))?;
    match docs.len() {
        1 => Ok(docs.remove(0)),
        n => Err(format!(
            "{}: expected exactly 1 document, got {n}",
            p.display()
        )),
    }
}

/// 特性值的两种形态：<v> 标量 enable|disable（全平台统一），或映射
/// {all: v, <os>-<arch>: v, ...}（先取本平台键、缺省回退 all，两者皆
/// 无 → None = 本平台未声明）。映射内平台名限已知五键+all，且所有
/// 内部值先过 enable|disable 校验——别平台行的坏值当场报错，不闷
/// 到轮值才炸 / the two value shapes of a feature: a scalar
/// enable|disable (uniform across platforms), or a map
/// {all: v, <os>-<arch>: v, ...} (this platform's key first, `all` as
/// fallback; neither → None = undeclared here). Inner platform keys
/// are restricted to the five known names plus `all`, and every inner
/// value is validated up front — a bad value on another platform's
/// line fails loudly here instead of waiting for that leg
fn pick_value(v: &Yaml, name: &str, file: &str, plat: &str) -> Result<Option<bool>, String> {
    const PLATS: &[&str] = &[
        "windows-x86_64",
        "windows-arm64",
        "linux-x86_64",
        "linux-arm64",
        "macos-arm64",
    ];
    let onoff = |s: &str, ctx: &str| -> Result<bool, String> {
        match s {
            "enable" => Ok(true),
            "disable" => Ok(false),
            other => Err(format!(
                "{file}: '{ctx}' must be enable or disable, got '{other}'"
            )),
        }
    };
    if let Some(s) = v.as_str() {
        return onoff(s, name).map(Some);
    }
    let h = v
        .as_hash()
        .ok_or_else(|| format!("{file}: '{name}' must be enable|disable or a platform map"))?;
    for (pk, pv) in h {
        let pkey = pk
            .as_str()
            .ok_or_else(|| format!("{file}: '{name}' has a non-string platform key"))?;
        if pkey != "all" && !PLATS.contains(&pkey) {
            return Err(format!("{file}: '{name}' has unknown platform '{pkey}'"));
        }
        // 全体校验，不止选中值 / validate every entry, not only the pick
        onoff(
            pv.as_str()
                .ok_or_else(|| format!("{file}: '{name}.{pkey}' is not a string"))?,
            &format!("{name}.{pkey}"),
        )?;
    }
    match v[plat].as_str().or(v["all"].as_str()) {
        Some(s) => onoff(s, name).map(Some),
        None => Ok(None),
    }
}

/// 从 {name: value} 映射 <doc> 里取键值并经 <insert> 插入合并表（<file>
/// 为错误定位用清单名；本平台未声明的键跳过）
/// pull entries out of the {name: value} map <doc> into the merged table
/// via <insert> (<file> is the manifest name used in error messages; keys
/// undeclared for this platform are skipped)
fn absorb_map(
    doc: &Yaml,
    file: &str,
    plat: &str,
    insert: &mut impl FnMut(&str, bool, bool, &str) -> Result<(), String>,
) -> Result<(), String> {
    let h = doc
        .as_hash()
        .ok_or_else(|| format!("{file}: expected a name: value mapping"))?;
    for (k, v) in h {
        let name = k
            .as_str()
            .ok_or_else(|| format!("{file}: feature key must be a string"))?;
        if let Some(en) = pick_value(v, name, file, plat)? {
            insert(name, en, false, file)?;
        }
    }
    Ok(())
}

/// 载入并解析当前平台的特性集
/// Load and resolve the feature set for the current platform
pub fn resolve() -> Result<Resolved, String> {
    let e = sys::env();
    let dir = e.repo_root.join("features");
    let plat = e.plat();
    // (name, enable, base) 合并表；同一键出现两次直接报错
    // merged (name, enable, base); any duplicate key is a hard error
    let mut map: BTreeMap<String, (bool, bool)> = BTreeMap::new();

    let mut insert = |name: &str, enable: bool, base: bool, file: &str| -> Result<(), String> {
        if !FEATURES.iter().any(|x| x.name == name) {
            return Err(format!("{file}: unknown feature '{name}'"));
        }
        if map.insert(name.to_string(), (enable, base)).is_some() {
            return Err(format!("{file}: duplicate feature key '{name}'"));
        }
        Ok(())
    };

    // base.yaml：字符串数组，恒 enable / base.yaml: string array, always on
    let base_doc = load_yaml(&dir.join("base.yaml"))?;
    let names = base_doc
        .as_vec()
        .ok_or("base.yaml: expected a string array")?;
    for n in names {
        let n = n.as_str().ok_or("base.yaml: entries must be strings")?;
        insert(n, true, true, "base.yaml")?;
    }

    // common.yaml/misc.yaml/平台文件同享"标量 | {all: v, <os>-<arch>: v}"
    // 值形态——差异只在职责：common 放多平台共享项，misc 放稀有可选项，
    // 平台文件放本平台独有项 / common.yaml, misc.yaml and the platform
    // file share the same scalar|platform-map value shapes — they differ
    // only in duty: common holds multi-platform shared items, misc the
    // rare optional ones, platform files the platform-exclusive ones
    absorb_map(
        &load_yaml(&dir.join("common.yaml"))?,
        "common.yaml",
        &plat,
        &mut insert,
    )?;

    absorb_map(
        &load_yaml(&dir.join("misc.yaml"))?,
        "misc.yaml",
        &plat,
        &mut insert,
    )?;

    // 平台文件可缺省 / platform file is optional
    let pfile = dir.join(format!("{plat}.yaml"));
    if pfile.is_file() {
        absorb_map(
            &load_yaml(&pfile)?,
            &format!("{plat}.yaml"),
            &plat,
            &mut insert,
        )?;
    }

    let mut r = Resolved {
        flags: Vec::new(),
        enabled: Vec::new(),
        degraded: Vec::new(),
    };
    for ft in FEATURES {
        match map.get(ft.name) {
            None => {
                // 默认全 disable / strict default-disable
                r.flags.push(format!("--disable-{}", ft.name));
            }
            Some((_, true)) => {
                // base：恒 enable，不探测 / base: unconditional enable
                r.flags.push(format!("--enable-{}", ft.name));
                r.enabled.push(ft.name.to_string());
            }
            Some((true, false)) => match pkgcfg::check_spec(ft.probe) {
                Ok(()) => {
                    r.flags.push(format!("--enable-{}", ft.name));
                    r.enabled.push(ft.name.to_string());
                }
                Err(e) => {
                    log::warn(&format!(
                        "feature {}: probe failed, degrading to disable ({e})",
                        ft.name
                    ));
                    r.flags.push(format!("--disable-{}", ft.name));
                    r.degraded.push(ft.name.to_string());
                }
            },
            Some((false, false)) => {
                r.flags.push(format!("--disable-{}", ft.name));
            }
        }
    }
    Ok(r)
}

/// `ffbuild features` 调试子命令：打印解析结果
/// Debug subcommand: print the resolved feature set
pub fn dump() -> Result<(), String> {
    let r = resolve()?;
    println!("enabled ({}):", r.enabled.len());
    for n in &r.enabled {
        println!("  {n}");
    }
    if !r.degraded.is_empty() {
        println!("degraded ({}):", r.degraded.len());
        for n in &r.degraded {
            println!("  {n}");
        }
    }
    println!("flags ({}):", r.flags.len());
    for x in &r.flags {
        println!("  {x}");
    }
    Ok(())
}

/// features 目录路径 / path of the features directory
pub fn dir() -> std::path::PathBuf {
    sys::env().repo_root.join("features")
}
