//! 冒烟测试：对解压后的 FFmpeg 构建做体检
//! Smoke test: sanity-check an extracted FFmpeg build
//! 在每个发布的平台上运行，只依赖 ffmpeg/ffprobe 二进制本身。
//! Runs on every shipped platform; relies only on the ffmpeg/ffprobe
//! binaries themselves.
//!
//! 覆盖面由 features 清单驱动：resolve() 的 enabled 集（清单意图 × 库
//! 探测）就是"本构建该有哪些特性"的期望——每个启用特性都要 (a) 在
//! -version 里看到 --enable 旗标（直抓 configure 丢包）(b) 有对应组件
//! 出现在 ffmpeg 的组件清单里（presence）(c) 能空跑的真实跑一遍
//! （functional）/ Coverage is driven by the features manifest:
//! resolve()'s enabled set (manifest intent × library probes) is the
//! expectation of what this build should contain — every enabled feature
//! must (a) show its --enable flag in -version (catches configure drops
//! directly), (b) have a component in ffmpeg's listings (presence), and
//! (c) actually run when a self-contained exercise exists (functional).

use crate::log;
use crate::sys;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

/// annotate_error：发 GitHub 错误注解（换行/百分号按 workflow-command
/// 语法转义）/ emit a GitHub error annotation (newlines/percent escaped
/// for the workflow-command syntax)
fn annotate_error(msg: &str) {
    let m = msg
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    println!("::error title=smoke test::{m}");
}
fn annotate_warn(msg: &str) {
    let m = msg
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    println!("::warning title=smoke test::{m}");
}

/// 每个发布构建都必须有的编码器 / encoders every shipped build must have
const REQUIRED_ENCODERS: &[&str] = &["libx264", "libopus", "libmp3lame"];

/// 组件清单类别：对应 ffmpeg -< listing> 的哪个输出 / which ffmpeg
/// -<listing> output a component appears in
#[derive(Clone, Copy)]
enum L {
    Enc,
    Dec,
    Filt,
    Prot,
    Demux,
    Mux,
    Dev,
    Hw,
}
impl L {
    fn flag(self) -> &'static str {
        match self {
            L::Enc => "-encoders",
            L::Dec => "-decoders",
            L::Filt => "-filters",
            L::Prot => "-protocols",
            L::Demux => "-demuxers",
            L::Mux => "-muxers",
            L::Dev => "-devices",
            L::Hw => "-hwaccels",
        }
    }
    fn name(self) -> &'static str {
        match self {
            L::Enc => "encoders",
            L::Dec => "decoders",
            L::Filt => "filters",
            L::Prot => "protocols",
            L::Demux => "demuxers",
            L::Mux => "muxers",
            L::Dev => "devices",
            L::Hw => "hwaccels",
        }
    }
}

/// presence 表：yaml 特性名 → (清单, 候选组件 any-of)。组件名以清单输出
/// 的空白分隔 token 精确匹配（protocols 项带冒号，抓取时已修剪）。
/// 特性可占多行（dvdnav/dvdread 同指 dvdvideo）/ presence table:
/// yaml feature -> (listing, any-of component candidates). Component
/// names match whitespace-separated tokens exactly (protocol entries
/// carry colons, trimmed at capture). A feature may take several rows
/// (dvdnav/dvdread both expect dvdvideo)
const PRESENCE: &[(&str, L, &[&str])] = &[
    // ---- 视频编解码 / video codecs ----
    ("libx264", L::Enc, &["libx264", "libx264rgb"]),
    ("libx265", L::Enc, &["libx265"]),
    ("libaom", L::Enc, &["libaom-av1"]),
    ("libdav1d", L::Dec, &["libdav1d"]),
    ("libvpx", L::Enc, &["libvpx-vp8", "libvpx-vp9"]),
    ("libsvtav1", L::Enc, &["libsvtav1"]),
    ("librav1e", L::Enc, &["librav1e"]),
    ("libjxl", L::Enc, &["libjxl"]),
    ("libwebp", L::Enc, &["libwebp", "libwebp_anim"]),
    ("libopenjpeg", L::Enc, &["libopenjpeg"]),
    ("libtheora", L::Enc, &["libtheora"]),
    ("libxvid", L::Enc, &["libxvid"]),
    ("libkvazaar", L::Enc, &["libkvazaar"]),
    ("libopenh264", L::Enc, &["libopenh264"]),
    ("liboapv", L::Enc, &["liboapv"]),
    ("libxeve", L::Enc, &["libxeve"]),
    ("libxevd", L::Dec, &["libxevd"]),
    ("libvvenc", L::Enc, &["libvvenc"]),
    ("libdavs2", L::Dec, &["libdavs2"]),
    ("libxavs2", L::Enc, &["libxavs2"]),
    ("libsvtjpegxs", L::Enc, &["libsvtjpegxs"]),
    ("libsvtjpegxs", L::Dec, &["libsvtjpegxs"]),
    // FFmpeg 8 起 liblcevc_dec 的组件面是 lcevc 滤镜（背后是 lcevc
    // 解复用/复用/lcevc_merge bsf 一族）——旧的 liblcevc_* 解码器族
    // 已移除；滤镜是最稳的单一注册名 / since FFmpeg 8, liblcevc_dec's
    // component face is the `lcevc` filter (backed by the lcevc
    // demuxer/muxer/lcevc_merge bsf family) — the old liblcevc_*
    // decoder family is gone; the filter is the stable single name
    ("liblcevc-dec", L::Filt, &["lcevc"]),
    ("libuavs3d", L::Dec, &["libuavs3d"]),
    (
        "libvpl",
        L::Enc,
        &["h264_qsv", "hevc_qsv", "av1_qsv", "mpeg2_qsv", "vp9_qsv"],
    ),
    ("libmfx", L::Enc, &["h264_qsv", "hevc_qsv", "av1_qsv"]),
    // ---- 音频编解码 / audio codecs ----
    ("libvorbis", L::Enc, &["libvorbis"]),
    ("libopus", L::Enc, &["libopus"]),
    ("libmp3lame", L::Enc, &["libmp3lame"]),
    ("libspeex", L::Enc, &["libspeex"]),
    ("libshine", L::Enc, &["libshine"]),
    ("libgsm", L::Enc, &["libgsm", "libgsm_ms"]),
    ("liblc3", L::Enc, &["liblc3"]),
    ("libopencore-amrnb", L::Enc, &["libopencore_amrnb"]),
    ("libopencore-amrwb", L::Dec, &["libopencore_amrwb"]),
    ("libvo-amrwbenc", L::Enc, &["libvo_amrwbenc"]),
    ("libtwolame", L::Enc, &["libtwolame"]),
    ("libcodec2", L::Enc, &["libcodec2"]),
    ("libilbc", L::Enc, &["libilbc"]),
    ("libfdk-aac", L::Enc, &["libfdk_aac"]),
    // tracker 格式三家是解复用器（libavformat），不是解码器——解码由
    // 库自带渲染管线产 pcm / the three tracker-format libs are
    // demuxers (libavformat), not decoders — their own render
    // pipelines emit pcm
    ("libmodplug", L::Demux, &["libmodplug"]),
    ("libopenmpt", L::Demux, &["libopenmpt"]),
    ("libgme", L::Demux, &["libgme"]),
    ("openal", L::Dev, &["openal"]),
    // ---- 滤镜 / filters ----
    ("libflite", L::Filt, &["flite"]),
    ("pocketsphinx", L::Filt, &["asr"]),
    ("libbs2b", L::Filt, &["bs2b"]),
    ("librubberband", L::Filt, &["rubberband"]),
    ("libmysofa", L::Filt, &["sofalizer"]),
    ("libass", L::Filt, &["ass", "subtitles"]),
    // drawtext 只在 freetype 在场时才编译——它就是 freetype 的组件面 /
    // drawtext only compiles when freetype is present — it IS
    // freetype's component face
    ("libfreetype", L::Filt, &["drawtext"]),
    // lcms2 的滤镜面是 iccdetect/iccgen（configure: *_filter_deps
    // ="lcms2"）——没有叫 icc_transform 的东西 / lcms2's filter face
    // is iccdetect/iccgen (configure: *_filter_deps="lcms2") — there
    // is no such thing as icc_transform
    ("lcms2", L::Filt, &["iccdetect", "iccgen"]),
    ("cairo", L::Filt, &["drawvg"]),
    ("librsvg", L::Dec, &["librsvg"]),
    ("libzimg", L::Filt, &["zscale"]),
    (
        "libvidstab",
        L::Filt,
        &["vidstabdetect", "vidstabtransform"],
    ),
    ("libvmaf", L::Filt, &["libvmaf"]),
    ("frei0r", L::Filt, &["frei0r", "frei0r_src"]),
    ("libtesseract", L::Filt, &["ocr"]),
    ("libqrencode", L::Filt, &["qrencode", "qrencodesrc"]),
    ("libquirc", L::Filt, &["quirc"]),
    ("liblensfun", L::Filt, &["lensfun"]),
    ("whisper", L::Filt, &["whisper"]),
    ("lv2", L::Filt, &["lv2"]),
    ("ladspa", L::Filt, &["ladspa"]),
    ("libplacebo", L::Filt, &["libplacebo"]),
    ("libopencv", L::Filt, &["ocv"]),
    (
        "cuda-llvm",
        L::Filt,
        &[
            "scale_cuda",
            "yadif_cuda",
            "bilateral_cuda",
            "chromakey_cuda",
            "colorspace_cuda",
            "overlay_cuda",
            "thumbnail_cuda",
            "transpose_cuda",
        ],
    ),
    ("coreimage", L::Filt, &["coreimage", "coreimagesrc"]),
    // ---- 字幕解码 / subtitle decoders ----
    ("libaribcaption", L::Dec, &["libaribcaption"]),
    ("libaribb24", L::Dec, &["libaribb24"]),
    // 注册名是 libzvbi_teletextdec——前缀兜底 / registers as
    // libzvbi_teletextdec — prefix fallback
    ("libzvbi", L::Dec, &["libzvbi_"]),
    // ---- 容器、协议、光盘 / containers, protocols, discs ----
    ("libxml2", L::Demux, &["dash"]),
    ("libssh", L::Prot, &["sftp", "scp", "libssh"]),
    ("libsrt", L::Prot, &["srt"]),
    ("librist", L::Prot, &["rist"]),
    ("libzmq", L::Prot, &["zmq"]),
    ("librabbitmq", L::Prot, &["amqp"]),
    ("libbluray", L::Prot, &["bluray"]),
    // dvdvideo 解复用器由 dvdnav+dvdread 合力支撑 /
    // the dvdvideo demuxer stands on dvdnav+dvdread together
    ("libdvdnav", L::Demux, &["dvdvideo"]),
    ("libdvdread", L::Demux, &["dvdvideo"]),
    ("libcdio", L::Dev, &["libcdio"]),
    ("libcaca", L::Dev, &["caca"]),
    ("chromaprint", L::Mux, &["chromaprint"]),
    ("libsmbclient", L::Prot, &["smb"]),
    ("avisynth", L::Demux, &["avisynth"]),
    ("vapoursynth", L::Demux, &["vapoursynth"]),
    // ---- 硬件加速 / hwaccel ----
    ("vaapi", L::Hw, &["vaapi"]),
    ("vdpau", L::Hw, &["vdpau"]),
    ("opencl", L::Hw, &["opencl"]),
    ("vulkan", L::Hw, &["vulkan"]),
    ("nvdec", L::Hw, &["cuda"]),
    (
        "cuvid",
        L::Dec,
        &[
            "h264_cuvid",
            "hevc_cuvid",
            "av1_cuvid",
            "vp9_cuvid",
            "vp8_cuvid",
            "mjpeg_cuvid",
        ],
    ),
    ("nvenc", L::Enc, &["h264_nvenc", "hevc_nvenc", "av1_nvenc"]),
    (
        "v4l2_m2m",
        L::Enc,
        &[
            "h264_v4l2m2m",
            "hevc_v4l2m2m",
            "vp8_v4l2m2m",
            "vp9_v4l2m2m",
            "mpeg4_v4l2m2m",
            "mpeg2_v4l2m2m",
        ],
    ),
    ("d3d11va", L::Hw, &["d3d11va"]),
    ("d3d12va", L::Hw, &["d3d12va"]),
    ("dxva2", L::Hw, &["dxva2"]),
    ("videotoolbox", L::Hw, &["videotoolbox"]),
    (
        "mediafoundation",
        L::Enc,
        &["h264_mf", "hevc_mf", "av1_mf", "aac_mf", "mp3_mf"],
    ),
    ("amf", L::Enc, &["h264_amf", "hevc_amf", "av1_amf"]),
    (
        "audiotoolbox",
        L::Enc,
        &["aac_at", "alac_at", "ac3_at", "eac3_at", "mp3_at"],
    ),
    // ---- 采集设备 / capture devices ----
    ("alsa", L::Dev, &["alsa"]),
    ("libpulse", L::Dev, &["pulse"]),
    ("libdc1394", L::Dev, &["libdc1394"]),
    ("xlib", L::Dev, &["x11grab"]),
    // xcbgrab.c 注册的 demuxer 名是 x11grab（xcb 后端顶替 Xlib
    // 同名 indev——源码 p.name 实锤）/ xcbgrab.c registers the demuxer
    // name `x11grab` (the xcb backend takes over the Xlib indev's
    // name — p.name in the source is definitive)
    ("libxcb", L::Dev, &["x11grab"]),
    ("libdrm", L::Dev, &["kmsgrab"]),
    ("avfoundation", L::Dev, &["avfoundation"]),
    ("decklink", L::Dev, &["decklink"]),
    ("libjack", L::Dev, &["jack"]),
    ("sndio", L::Dev, &["sndio"]),
    // ---- 杂项组件面 / misc component faces ----
    // --enable-zlib 会注册 zlib 解码器（LCL ZLIB codec）/
    // --enable-zlib registers the zlib decoder (the LCL ZLIB codec)
    ("zlib", L::Dec, &["zlib"]),
];

/// 无独立组件面的特性（TLS 后端、字体整形、gmp、封装头文件等）——
/// presence 到 --enable 旗标为止 / features with no distinct component
/// face (TLS backends, text shaping, gmp, header shims, ...) — presence
/// stops at the --enable flag
const FLAG_ONLY: &[&str] = &[
    "gnutls",
    "openssl",
    "mbedtls",
    "gcrypt",
    "schannel",
    "securetransport",
    "libharfbuzz",
    "libfribidi",
    "libfontconfig",
    "gmp",
    "libsnappy",
    "libcurl",
    "librtmp",
    "ffnvcodec",
    "cuda-nvcc",
    "appkit",
    "metal",
    "libklvanc",
    "libopenvino",
    "libtensorflow",
    "libtorch",
    "libxcb-shm",
    "libxcb-shape",
    "libxcb-xfixes",
    // sdl2 上游已移除 outdev（master 的 outdev_list.c 无 sdl 条目），
    // 现在只喂 ffplay_deps——唯一可观察面是 --enable 旗标本身（ffplay
    // 存在性由必需二进制检查覆盖）/ sdl2's outdev was removed upstream
    // (no sdl entry in master's outdev_list.c) — it now only feeds
    // ffplay_deps; the flag itself is the only observable face (ffplay's
    // existence is covered by the required-bins check)
    "sdl2",
    // 压缩/字符集底座——不注册任何可名列的组件 / compression/charset
    // plumbing — no nameable component face
    "bzlib",
    "lzma",
    "iconv",
];

/// 功能演练种类 / functional exercise kinds
#[derive(Clone, Copy)]
enum Fkind {
    /// testsrc2 → -c:v <comp>（extra 插在 -c:v 前）
    /// testsrc2 -> -c:v <comp> (extra spliced before -c:v)
    VEnc,
    /// sine → -c:a <comp>（extra 插 -c:a 前，携带采样率等约束）
    /// sine -> -c:a <comp> (extra carries rate/channel constraints)
    AEnc,
    /// testsrc2 → -vf <comp=filtergraph> / testsrc2 -> -vf <graph>
    VFilter,
    /// sine → -af <comp=filtergraph> / sine -> -af <graph>
    AFilter,
    /// lavfi 源 <comp> 直接输入 / lavfi source <comp> as the input
    LavfiSrcV,
    LavfiSrcA,
    /// sine → -f chromaprint 实写文件 / sine -> -f chromaprint real file
    MuxFile,
}

/// (yaml 特性名, 组件名或 filtergraph, 种类, 附加参数) /
/// (yaml feature, component or filtergraph, kind, extra args)
const FUNCTIONAL: &[(&str, &str, Fkind, &[&str])] = &[
    ("libx264", "libx264", Fkind::VEnc, &[]),
    ("libx265", "libx265", Fkind::VEnc, &[]),
    ("libvpx", "libvpx-vp9", Fkind::VEnc, &[]),
    ("libsvtav1", "libsvtav1", Fkind::VEnc, &[]),
    ("libaom", "libaom-av1", Fkind::VEnc, &[]),
    ("librav1e", "librav1e", Fkind::VEnc, &[]),
    ("libkvazaar", "libkvazaar", Fkind::VEnc, &[]),
    ("libxvid", "libxvid", Fkind::VEnc, &[]),
    ("libtheora", "libtheora", Fkind::VEnc, &[]),
    ("libopenh264", "libopenh264", Fkind::VEnc, &[]),
    ("libxeve", "libxeve", Fkind::VEnc, &[]),
    ("libvvenc", "libvvenc", Fkind::VEnc, &[]),
    ("libxavs2", "libxavs2", Fkind::VEnc, &[]),
    ("libwebp", "libwebp", Fkind::VEnc, &[]),
    ("libopus", "libopus", Fkind::AEnc, &["-ar", "48000"]),
    ("libmp3lame", "libmp3lame", Fkind::AEnc, &[]),
    ("libvorbis", "libvorbis", Fkind::AEnc, &[]),
    ("libtwolame", "libtwolame", Fkind::AEnc, &[]),
    ("libshine", "libshine", Fkind::AEnc, &[]),
    (
        "libopencore-amrnb",
        "libopencore_amrnb",
        Fkind::AEnc,
        &["-ar", "8000", "-ac", "1"],
    ),
    (
        "libvo-amrwbenc",
        "libvo_amrwbenc",
        Fkind::AEnc,
        &["-ar", "16000", "-ac", "1"],
    ),
    (
        "libgsm",
        "libgsm",
        Fkind::AEnc,
        &["-ar", "8000", "-ac", "1"],
    ),
    (
        "libcodec2",
        "libcodec2",
        Fkind::AEnc,
        &["-ar", "8000", "-ac", "1"],
    ),
    (
        "libilbc",
        "libilbc",
        Fkind::AEnc,
        &["-ar", "8000", "-ac", "1"],
    ),
    (
        "libspeex",
        "libspeex",
        Fkind::AEnc,
        &["-ar", "8000", "-ac", "1"],
    ),
    ("liblc3", "liblc3", Fkind::AEnc, &[]),
    ("libzimg", "zscale=w=320:h=240", Fkind::VFilter, &[]),
    ("libbs2b", "bs2b", Fkind::AFilter, &[]),
    ("librubberband", "rubberband=tempo=1.1", Fkind::AFilter, &[]),
    // soxr 没有独立组件面——aresample=resampler=soxr 就是它的功能位 /
    // soxr has no component face of its own — aresample=resampler=soxr
    // IS its functional slot
    ("libsoxr", "aresample=resampler=soxr", Fkind::AFilter, &[]),
    ("libflite", "flite=text=hello", Fkind::LavfiSrcA, &[]),
    (
        "libqrencode",
        "qrencodesrc=text=ffbuild",
        Fkind::LavfiSrcV,
        &[],
    ),
    ("chromaprint", "chromaprint", Fkind::MuxFile, &[]),
];

/// (编码器, 解码器, 中间格式) —— 编码 5 帧落地再由对应第三方解码器解出
/// 的 roundtrip / (encoder, decoder, container) — encode 5 frames to
/// disk then decode back through the paired third-party decoder
const ROUNDTRIP: &[(&str, &str, &str)] = &[
    ("libaom-av1", "libdav1d", "ivf"),
    ("libxeve", "libxevd", "evc"),
    ("libxavs2", "libdavs2", "avs2"),
];

/// token 集合：清单输出按空白切，protocols 尾冒号一并修剪 /
/// token set: split listings on whitespace, trimming the protocol colons
fn token_set(out: &str) -> HashSet<String> {
    out.split_whitespace()
        .map(|t| t.trim_matches(':'))
        .filter(|t| !t.is_empty() && !t.starts_with('-') && t.len() > 1)
        .map(String::from)
        .collect()
}

/// 清单 token 命中：候选串以 _ 结尾按前缀匹配（liblcevc_ 族），否则精确 /
/// listing token hit: a candidate ending in _ matches by prefix (the
/// liblcevc_ family), others match exactly
fn listed(set: &HashSet<String>, cands: &[&str]) -> Option<String> {
    cands.iter().find_map(|c| {
        if let Some(pre) = c.strip_suffix('_') {
            if set.iter().any(|t| t.starts_with(pre)) {
                return Some((*c).to_string());
            }
            None
        } else if set.contains(*c) {
            Some((*c).to_string())
        } else {
            None
        }
    })
}

/// 视频编码失败的诊断链：SIMD 掩码 → 单线程 → gdb 回溯（沿用旧行为，
/// 原 TRIED_ENCODERS 失败路径整段抽出）/ the failed-encode diagnostic
/// chain: SIMD mask -> single thread -> gdb backtrace (carried over
/// from the old TRIED_ENCODERS failure path, extracted verbatim)
fn diagnose_failed_encode(fm: &str, enc: &str, args: &[&str], bin: &Path, rc: i32) {
    // SIMD 掩码诊断：libaom 读 AOM_SIMD_CAPS_MASK——0 掩掉全部 SIMD
    // 档回落纯 C，过即"SIMD 路径崩"反证（libaom-av1 在 avx512 平台
    // 的 0xC0000005 实证）。其它编码器不吃这个变量，无副作用 /
    // SIMD-mask diagnostic: libaom honors AOM_SIMD_CAPS_MASK — 0
    // masks every SIMD level onto the C path, so a pass is
    // counter-evidence for a SIMD-path crash (empirical:
    // libaom-av1's 0xC0000005 on an avx512 host). Other encoders
    // ignore the var — no side effects
    let drc = sys::cmd(fm, args)
        .env("AOM_SIMD_CAPS_MASK", "0")
        .status()
        .map(|s| s.code().unwrap_or(-1))
        .unwrap_or(1);
    println!("encode test: {enc} retried with AOM_SIMD_CAPS_MASK=0 -> exit {drc}");
    // 单线程诊断：enc_init 里按核数配 worker/线程资源——高核数
    // 机器上 av1 的 >64 逻辑核路径（processor groups）有崩溃先例；
    // -threads 1 过即 MT 分配是死因，仍崩则锁定纯 init 路径 /
    // single-thread diagnostic: enc_init sizes workers/thread
    // resources by core count — av1 has a >64-logical-core crash
    // precedent (processor groups); a -threads 1 pass fingers MT
    // allocation, a continued crash pins pure init
    let mut targs: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    // args 里 -c:v <enc> 前插 -threads 1 / splice -threads 1 before
    // -c:v <enc>
    if let Some(i) = targs.iter().position(|s| s == "-c:v") {
        targs.splice(i..i, ["-threads".into(), "1".into()]);
        let trc = sys::run(
            fm,
            &targs.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            bin,
        )
        .unwrap_or(1);
        println!("encode test: {enc} retried with -threads 1 -> exit {trc}");
    }
    // 崩溃（132=SIGILL，139=SIGSEGV）且无输出时：PATH 上有 gdb 时在
    // gdb 下重跑抓回溯。打包后的 $FFMPEG strip 过——FFMPEG_GDB 指
    // 未剥离副本 / on a crash with no output, rerun under gdb when one
    // is on PATH: the packaged ffmpeg is stripped, FFMPEG_GDB points at
    // the unstripped copy
    // Windows 崩溃走 NTSTATUS 负码（0xC0000005 即 -1073741819），
    // 不是 POSIX 信号值——负数退出码一并算崩溃 / Windows crashes
    // surface as negative NTSTATUS codes (0xC0000005 ==
    // -1073741819), not POSIX signal values — any negative exit
    // code counts as a crash
    if (rc == 132 || rc == 139 || rc < 0) && sys::which("gdb").is_some() {
        log::group_begin(&format!("gdb backtrace: {enc}"));
        let target = std::env::var("FFMPEG_GDB").unwrap_or_else(|_| fm.to_string());
        // 全文落 dist 诊断件——CI output gate 会把大输出吞掉（实证：
        // 25KiB 回溯只剩一行 suppressed 提示），控制台只留尾部精要 /
        // the full trace goes to a dist diagnostics file — the CI
        // output gate eats large output (empirical: a 25KiB backtrace
        // shrank to a one-line suppression note); the console keeps
        // only a tail excerpt
        let (trace, _) = sys::capture_lax(
            "gdb",
            &[
                "-q",
                "-batch",
                "-ex",
                "set confirm off",
                "-ex",
                "run",
                "-ex",
                "info threads",
                "-ex",
                "info registers",
                "-ex",
                "info sharedlibrary",
                // bt 压轴：控制台只打尾部 50 行——把帧序列放最后，
                // 否则尾窗被 info sharedlibrary 的 DLL 表整个占掉
                // / bt goes last: the console shows only the final
                // 50 lines — frames must sit at the tail or the DLL
                // table of info sharedlibrary fills the window
                "-ex",
                "thread apply all bt 30",
                "-ex",
                "bt 30",
                "-ex",
                "quit",
                "--args",
                &target,
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=25",
                "-frames:v",
                "5",
                "-c:v",
                enc,
                "-f",
                "null",
                "-",
            ],
            bin,
        )
        .unwrap_or_default();
        let dist = sys::env().dist.clone();
        let diag = dist.join(format!("smoke-gdb-{enc}.txt"));
        let _ = crate::fsx::install_d(&dist);
        let _ = crate::fsx::write(&diag, &trace);
        println!("[smoke] full gdb trace -> {}", diag.display());
        let tail: Vec<&str> = trace.lines().collect();
        for l in tail.iter().skip(tail.len().saturating_sub(50)) {
            println!("{l}");
        }
        log::group_end();
    }
}

/// smoke_test <bin-dir>：版本/许可证/特性覆盖/真实编码体检
/// Version/license/feature-coverage/real-encode checks
pub fn smoke_test(bin: &Path) -> Result<(), String> {
    let ext = if bin.join("ffmpeg.exe").is_file() {
        ".exe"
    } else {
        ""
    };
    let ffmpeg = bin.join(format!("ffmpeg{ext}"));
    let ffprobe = bin.join(format!("ffprobe{ext}"));
    let fm = ffmpeg.to_string_lossy().to_string();
    let fp = ffprobe.to_string_lossy().to_string();
    let fail = |msg: &str| -> Result<(), String> {
        annotate_error(&format!("smoke test failed: {msg}"));
        Err(format!("smoke test failed: {msg}"))
    };
    if !ffmpeg.is_file() {
        return fail(&format!("{} not found", ffmpeg.display()));
    }
    if !ffprobe.is_file() {
        return fail(&format!("{} not found", ffprobe.display()));
    }

    log::group_begin("ffmpeg -version");
    let version = sys::capture(&fm, &["-hide_banner", "-version"], bin)
        .map_err(|e| format!("ffmpeg -version: {e}"))?;
    print!("{version}");
    log::group_end();

    // --- 许可证体检 / license sanity ---
    if !version.contains("--enable-gpl") {
        return fail("build is not configured with --enable-gpl");
    }
    if version.contains("--enable-nonfree") {
        return fail("build is configured with --enable-nonfree and must not be redistributed");
    }

    // --- 期望特性集：features 清单为本（resolve 的 enabled = 清单意图 ×
    // 库探测）；独立 smoke 无前缀可探时回落 -version 旗标 ∩ 已知组件面 /
    // expected feature set: the manifest is authoritative (resolve's
    // enabled = intent × probes); standalone smoke with no prefix to
    // probe falls back to -version flags ∩ known component faces
    let version_enabled: BTreeSet<String> = version
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("--enable-"))
        .map(String::from)
        .collect();
    let known: BTreeSet<&str> = PRESENCE
        .iter()
        .map(|r| r.0)
        .chain(FLAG_ONLY.iter().copied())
        .chain(FUNCTIONAL.iter().map(|r| r.0))
        .collect();
    let resolved = crate::features::resolve()
        .map(|r| {
            // 探测降级=预期缺席，仅信息输出（不是失败）/ probe-degraded
            // is expected-absent, informational only (not a failure)
            if !r.degraded.is_empty() {
                println!(
                    "manifest degraded (expected absent): {}",
                    r.degraded.join(" ")
                );
            }
            r.enabled.into_iter().collect::<BTreeSet<String>>()
        })
        .unwrap_or_default();
    let expected: BTreeSet<String> = if !resolved.is_empty() {
        resolved
    } else {
        version_enabled
            .iter()
            .filter(|n| known.contains(n.as_str()))
            .cloned()
            .collect()
    };

    // --- 外部库清单（沿用旧输出）/ external-library inventory ---
    let mut libs: Vec<&str> = version
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("--enable-lib"))
        .collect();
    libs.sort();
    libs.dedup();
    println!(
        "External libraries enabled ({}): lib{}",
        libs.len(),
        libs.join(" lib")
    );

    // --- 组件清单一次抓齐 / capture every listing once ---
    let mut listings: BTreeMap<&'static str, HashSet<String>> = BTreeMap::new();
    for l in [
        L::Enc,
        L::Dec,
        L::Filt,
        L::Prot,
        L::Demux,
        L::Mux,
        L::Dev,
        L::Hw,
    ] {
        let out = sys::capture(&fm, &["-hide_banner", l.flag()], bin)
            .map_err(|e| format!("ffmpeg {}: {e}", l.flag()))?;
        listings.insert(l.name(), token_set(&out));
    }
    let in_listing =
        |l: L, cands: &[&str]| -> Option<String> { listed(&listings[l.name()], cands) };

    // --- 必需编码器 / required encoders ---
    let encoders = &listings[L::Enc.name()];
    let has = |enc: &str| encoders.contains(enc);
    for enc in REQUIRED_ENCODERS {
        if !has(enc) {
            return fail(&format!("required encoder '{enc}' is missing"));
        }
    }

    // --- presence：每个启用特性 flag 必现 + 组件必在清单 /
    // presence: every enabled feature must show its flag AND a component
    let mut presence_fail: Vec<String> = Vec::new();
    let mut uncovered: Vec<String> = Vec::new();
    for feat in &expected {
        if !version_enabled.contains(feat) {
            annotate_error(&format!(
                "feature '{feat}' enabled by the manifest but --enable-{feat} absent from ffmpeg -version"
            ));
            presence_fail.push(format!("{feat} (flag dropped)"));
            continue;
        }
        let rows: Vec<_> = PRESENCE.iter().filter(|r| r.0 == feat.as_str()).collect();
        if rows.is_empty() {
            // FUNCTIONAL 表自身就是覆盖（libsoxr 这类无组件面特性）——
            // 只把真正无行无演练的上报覆盖缺口 / a FUNCTIONAL row is
            // coverage in itself (face-less features like libsoxr) —
            // only truly row-less + exercise-less features get reported
            if !FLAG_ONLY.contains(&feat.as_str())
                && !FUNCTIONAL.iter().any(|f| f.0 == feat.as_str())
            {
                uncovered.push(feat.clone());
            }
            continue;
        }
        for (_, l, cands) in rows {
            match in_listing(*l, cands) {
                Some(hit) => println!("presence: {feat} -> {hit} ({})", l.name()),
                None => {
                    annotate_error(&format!(
                        "feature '{feat}' is enabled but none of {cands:?} appears in -{}",
                        l.name()
                    ));
                    presence_fail.push(format!("{feat} (no {})", l.name()));
                }
            }
        }
    }
    if !uncovered.is_empty() {
        annotate_warn(&format!(
            "enabled features with no smoke-coverage row: {}",
            uncovered.join(" ")
        ));
        println!("coverage gap (warning only): {}", uncovered.join(" "));
    }

    // --- 真实编码 / real encodes ---
    let out = bin.join("smoke-test-output.mkv");
    let _del = scopeguard(&out);
    log::group_begin("mux test (libx264 + libopus + libmp3lame)");
    if sys::run(
        &fm,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=25:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "libopus",
            "-f",
            "matroska",
            &out.to_string_lossy(),
        ],
        bin,
    )? != 0
    {
        return fail("libx264/libopus encode failed");
    }
    let streams = sys::capture(
        &fp,
        &[
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
            &out.to_string_lossy(),
        ],
        bin,
    )
    .unwrap_or_default()
    .replace('\n', " ");
    println!("streams: {streams}");
    if !(streams.contains("h264") && streams.contains("opus")) {
        return fail(&format!("unexpected streams in output: {streams}"));
    }
    log::group_end();

    // --- 功能演练：presence 之上能空跑的每个特性都真实跑一遍 /
    // functional: every enabled feature with a self-contained exercise
    // runs it for real
    let mut failed: Vec<String> = presence_fail;
    for (feat, comp, kind, extra) in FUNCTIONAL {
        if !expected.contains(*feat) {
            continue;
        }
        // presence 已报缺的组件不再复跑（避免一错两报）/ a component
        // already reported missing is not exercised again (one bug,
        // one report)
        let comp_name = comp.split('=').next().unwrap_or(comp);
        let comp_present = match *kind {
            Fkind::VEnc => has(comp_name),
            Fkind::AEnc => has(comp_name),
            Fkind::VFilter | Fkind::AFilter => listings[L::Filt.name()].contains(comp_name),
            Fkind::LavfiSrcV | Fkind::LavfiSrcA => listings[L::Filt.name()].contains(comp_name),
            Fkind::MuxFile => listings[L::Mux.name()].contains(comp_name),
        };
        if !comp_present {
            continue;
        }
        let v_in: &[&str] = &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=25:duration=2",
        ];
        let a_in: &[&str] = &["-f", "lavfi", "-i", "sine=frequency=440:duration=2"];
        let head: &[&str] = &["-hide_banner", "-loglevel", "error", "-y"];
        let args: Vec<String> = match kind {
            Fkind::VEnc => head
                .iter()
                .chain(v_in)
                .chain(*extra)
                // CFR 归一输出时间戳——深 lookahead 编码器（vvenc/xavs2）
                // 按内部重排吐包，muxer 对短输入会报非单调 dts /
                // CFR-normalize output timestamps — deep-lookahead
                // encoders (vvenc/xavs2) emit packets reordered, and the
                // muxer reports non-monotonic dts on short inputs
                .chain(
                    [
                        "-frames:v",
                        "5",
                        "-c:v",
                        comp,
                        "-fps_mode",
                        "cfr",
                        "-f",
                        "null",
                        "-",
                    ]
                    .iter(),
                )
                .map(|s| s.to_string())
                .collect(),
            Fkind::AEnc => head
                .iter()
                .chain(a_in)
                .chain(*extra)
                .chain(["-t", "1", "-c:a", comp, "-f", "null", "-"].iter())
                .map(|s| s.to_string())
                .collect(),
            Fkind::VFilter => head
                .iter()
                .chain(v_in)
                .chain(["-vf", comp, "-frames:v", "5", "-f", "null", "-"].iter())
                .map(|s| s.to_string())
                .collect(),
            Fkind::AFilter => head
                .iter()
                .chain(a_in)
                .chain(["-af", comp, "-t", "1", "-f", "null", "-"].iter())
                .map(|s| s.to_string())
                .collect(),
            Fkind::LavfiSrcV => head
                .iter()
                .chain(["-f", "lavfi", "-i", comp].iter())
                .chain(["-frames:v", "1", "-f", "null", "-"].iter())
                .map(|s| s.to_string())
                .collect(),
            Fkind::LavfiSrcA => head
                .iter()
                .chain(["-f", "lavfi", "-i", comp].iter())
                .chain(["-t", "0.5", "-f", "null", "-"].iter())
                .map(|s| s.to_string())
                .collect(),
            Fkind::MuxFile => {
                let dst = bin.join(format!("smoke-{comp}.txt"));
                let _g = scopeguard(&dst);
                let dst_s = dst.to_string_lossy().to_string();
                let a: Vec<String> = head
                    .iter()
                    .chain(a_in)
                    .chain(["-t", "1", "-f", comp, &dst_s].iter())
                    .map(|s| s.to_string())
                    .collect();
                let rc = sys::run(&fm, &a.iter().map(|s| s.as_str()).collect::<Vec<_>>(), bin)
                    .unwrap_or(1);
                if rc != 0 || !dst.is_file() {
                    annotate_error(&format!("exercise '{feat}' ({comp}) failed (exit {rc})"));
                    failed.push(format!("{feat}/{comp}"));
                } else {
                    println!("exercise: {feat} -> {comp} ok");
                }
                continue;
            }
        };
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rc = sys::run(&fm, &refs, bin).unwrap_or(1);
        if rc == 0 {
            println!("exercise: {feat} -> {comp} ok");
            continue;
        }
        println!("exercise: {feat} -> {comp} FAILED (exit {rc})");
        if matches!(kind, Fkind::VEnc) {
            diagnose_failed_encode(&fm, comp, &refs, bin, rc);
        }
        annotate_error(&format!(
            "feature '{feat}' enabled but its exercise ({comp}) failed (exit {rc})"
        ));
        failed.push(format!("{feat}/{comp}"));
    }

    // --- 编码→解码 roundtrip：成对第三方组件各验一半 /
    // encode->decode roundtrips: paired third-party components each
    // prove one direction
    for (enc, dec, fmt) in ROUNDTRIP {
        if !(has(enc) && listings[L::Dec.name()].contains(*dec)) {
            continue;
        }
        let tmp = bin.join(format!("smoke-rt-{dec}.{fmt}"));
        let _g = scopeguard(&tmp);
        let tmp_s = tmp.to_string_lossy().to_string();
        // 60 帧：编码器 lookahead/帧缓冲在吐出首个包前要吃饱——xevd/
        // xavs2 这类深缓冲编码器只喂几帧会被全吃掉，文件变空，下游
        // 解码报"no stream"误导成解码器故障 / 60 frames: deep-lookahead
        // encoders (xevd, xavs2) must fill their frame buffers before
        // emitting a packet — a handful of frames gets swallowed whole,
        // lands an empty file, and the decoder side misreports it as a
        // "no stream" decode failure
        let eargs = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=25",
            "-frames:v",
            "60",
            "-c:v",
            enc,
            "-fps_mode",
            "cfr",
            "-f",
            fmt,
            &tmp_s,
        ];
        // exit 码写进报错——信号死（-1/13x）与正常错误退出的修法
        // 完全不同 / carry the exit code in the report — a signal
        // death (-1/13x) needs a very different fix than a clean
        // error exit
        let erc = sys::run(&fm, &eargs, bin).unwrap_or(1);
        if erc != 0 {
            annotate_error(&format!(
                "roundtrip: {enc} failed to encode {fmt} (exit {erc})"
            ));
            failed.push(format!("{enc}(rt)"));
            continue;
        }
        // 编码吐空包（lookahead 吃光/悄悄零输出）本身就是故障——在这
        // 报错比等下游"no stream"误报好诊断 / an encode that emitted
        // nothing (lookahead ate it all / silent zero output) is
        // itself the failure — reporting it here beats letting the
        // downstream "no stream" misreport
        if std::fs::metadata(&tmp)
            .map(|m| m.len() == 0)
            .unwrap_or(true)
        {
            annotate_error(&format!("roundtrip: {enc} produced an empty {fmt} stream"));
            failed.push(format!("{enc}(rt)"));
            continue;
        }
        // 输入端 -f 强指解复用器：裸 ES 容器（evc/avs2/ivf）不保证被
        // 探针识别——识别失败时解码输入 0 流，报"no stream"而非真因 /
        // force the demuxer with input -f: bare-ES containers
        // (evc/avs2/ivf) are not guaranteed to probe — a failed probe
        // yields a zero-stream input and a misleading "no stream" error
        let dargs = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            fmt,
            "-c:v",
            dec,
            "-i",
            &tmp_s,
            "-f",
            "null",
            "-",
        ];
        let drc = sys::run(&fm, &dargs, bin).unwrap_or(1);
        if drc != 0 {
            annotate_error(&format!(
                "roundtrip: {dec} failed to decode {enc} output (exit {drc})"
            ));
            failed.push(format!("{dec}(rt)"));
        } else {
            println!("roundtrip: {enc} -> {dec} ok");
        }
    }

    if !failed.is_empty() {
        return Err(format!("smoke test failed: {}", failed.join(" ")));
    }
    println!("smoke test passed");
    Ok(())
}

/// 离开作用域时删除文件的小守卫 / tiny guard that removes a file on drop
struct Guard(Option<std::path::PathBuf>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}
fn scopeguard(p: &Path) -> Guard {
    Guard(Some(p.to_path_buf()))
}
