//! 源码补丁表：patches/<源名>/ 下的 .patch 文件在构建期经 include_str!
//! 编入驱动——零运行时路径依赖（recipe-run 子进程与任意 CWD 行为一致）。
//! materialize 物化完 <wd>/src 后按源名查表、按序应用（patch -p1）。
//! 版本钉死：每行可带 pin——archive 源对 sha256、git 源对 ref 比对，
//! 不一致立即报错（版本变了补丁必须人工重新核验，不许静默套用旧补丁
//! 或静默跳过）；None 表示不钉，补丁自身的 context 匹配兜底。
//! 新补丁：往 patches/<源名>/ 放 .patch 文件并在此表登记一行；同一个源
//! 多个补丁按表内顺序应用。
//! Source patch table: .patch files under patches/<source-name>/ are
//! embedded into the driver at build time via include_str! — no runtime
//! path dependence (behaves identically in the recipe-run subprocess and
//! any cwd). After materialize lands <wd>/src it looks the source name up
//! here and applies the bodies in order (patch -p1).
//! Version pinning: each row may carry a pin — compared against sha256
//! for archive sources and ref for git sources; a mismatch fails loudly
//! (a version bump forces a human to re-verify the patch — never silently
//! apply or skip a stale one). None = unpinned, where the patch's own
//! context matching is the only guard.
//! To add a patch: drop the .patch into patches/<source-name>/ and
//! register one row; several patches for one source apply in table order.

use crate::sys;
use std::path::Path;

/// 一行补丁：源名 + 版本钉 + 补丁体 / one patch row: source name +
/// version pin + patch body
struct Row {
    name: &'static str,
    /// 钉死标识：与 archive 源的 sha256 或 git 源的 ref 比对；None = 不
    /// 校验（git 浮动源钉不了静态值，靠补丁 context 匹配兜底）
    /// pinned identity: compared against an archive source's sha256 or a
    /// git source's ref; None = unpinned (a floating git source can't be
    /// pinned statically — the patch's own context match is the guard)
    pin: Option<&'static str>,
    body: &'static str,
}

static TABLE: &[Row] = &[
    Row {
        name: "ffmpeg",
        // davs2 解码器把 info_level 硬编码为 0（全量输出）——davs2 自有
        // stderr 日志不受 ffmpeg -loglevel 控制，冒烟时被版本横幅/线程/
        // CU 警告刷屏；提到 ERROR 级只对齐语义，不改解码行为 /
        // the davs2 decoder hardcodes info_level=0 (full verbosity) —
        // davs2's own stderr log ignores ffmpeg -loglevel and floods the
        // smoke log with version banners/thread chatter/CU warnings;
        // raising to ERROR aligns semantics, no decode-behavior change
        pin: None,
        body: include_str!("../../patches/ffmpeg/0001-libdavs2-error-level.patch"),
    },
    Row {
        name: "ffmpeg",
        // libxavs2 包装器走弃用名 "FrameRate"（整数编码），xavs2 每开一次
        // 编码器就打 XAVS2_LOG_ERROR 级弃用告警；改用现代名 "fps"（浮点
        // 帧率直传）——与上游参数表对齐，ffmpeg 侧迟早也会这么改 /
        // the libxavs2 wrapper uses the deprecated "FrameRate" name (int
        // code); xavs2 prints an XAVS2_LOG_ERROR deprecation line on every
        // encoder open; switch to the modern "fps" name (float rate
        // passed through) — matches upstream's own parameter table and
        // where upstream ffmpeg will land eventually
        pin: None,
        body: include_str!("../../patches/ffmpeg/0002-libxavs2-fps-param.patch"),
    },
    Row {
        name: "ffmpeg",
        // xavs2 的 ThreadRows=0 是"auto→按核数"——linux 腿实证 WPP 行
        // 池 worker 在 intra RDO 里 SIGSEGV（dct_32x32_c ← cu_check_intra
        // ← compress_ctu_intra ← xavs2_lcu_row_write ←
        // proc_xavs2_threadpool_thread，ffmpeg.nm.txt 离线解析），
        // -threads 1 只拧 ThreadFrames 救不了行池。上游 2019 弃坑钉死
        // eae1e8b——把包装器的 lcu_row_threads 默认 0 翻成 1，默认路径
        // 走串行行编码；用户仍可显式 -lcu_row_threads N 开 WPP 自担风
        // 险。若串行仍崩，下趟冒烟会证伪"竞态在行池"这一判定 /
        // xavs2's ThreadRows=0 means "auto→ncores" — the linux leg
        // empirically SIGSEGVs inside the WPP row pool during intra RDO
        // (dct_32x32_c ← cu_check_intra ← compress_ctu_intra ←
        // xavs2_lcu_row_write ← proc_xavs2_threadpool_thread, resolved
        // offline via ffmpeg.nm.txt); -threads 1 only touches
        // ThreadFrames and cannot disarm the row pool. Upstream is dead
        // since 2019 (pinned eae1e8b) — flip the wrapper's lcu_row_threads
        // default 0→1 so the default path encodes rows serially; users
        // can still opt into WPP with an explicit -lcu_row_threads N at
        // their own risk. If serial still crashes, the next smoke run
        // falsifies the "race lives in the row pool" verdict
        pin: None,
        body: include_str!("../../patches/ffmpeg/0003-libxavs2-serial-lcu-rows.patch"),
    },
    Row {
        name: "gmp",
        // 对 gmp-6.3.0.tar.xz 钉死 sha256——源一换版本此补丁必须人工复核 /
        // pinned to gmp-6.3.0.tar.xz's sha256 — bumping the source forces a
        // human re-verification of this patch
        pin: Some("a3c2b80201b89e68616f4ad30bc66aee4927c3ce50e33929ca819d5c43538898"),
        body: include_str!("../../patches/gmp/0001-configure-c23-long-long-test.patch"),
    },
    Row {
        name: "zvbi",
        // git 浮动源没钉 ref——靠补丁自身的 context 匹配兜底 /
        // floating git source has no pinned ref — the patch's own context
        // match is the guard
        pin: None,
        body: include_str!("../../patches/zvbi/0001-drop-contrib-subdir.patch"),
    },
];

/// 校验补丁行 <row> 的钉死版本仍与 <name> 的清单一致
/// verify <row>'s pin still matches <name>'s manifest-declared source
/// identity
fn check_pin(row: &Row, name: &str) -> Result<(), String> {
    let Some(pin) = row.pin else {
        return Ok(());
    };
    let spec = super::sources::spec(name).map_err(|e| {
        format!("patch registered for '{name}' but its source spec is missing: {e}")
    })?;
    if spec.sha256.as_deref() == Some(pin) || spec.ref_.as_deref() == Some(pin) {
        return Ok(());
    }
    Err(format!(
        "patch for '{name}' is pinned to {pin} but the spec now declares \
         sha256={:?} ref={:?} — the source version moved; re-verify the \
         patch against it, then update the pin or drop the row",
        spec.sha256, spec.ref_
    ))
}

/// 在 <src> 树上按序应用源名 <name> 的全部补丁（patch -p1 --forward；
/// 补丁打在 wd 副本上，src-cache 保持原始检出）。失败即报错——wd 每轮
/// 全新物化，补丁理应干净命中。
/// Apply every patch registered for <name> onto the <src> tree in order
/// (patch -p1 --forward; patches land on the wd copy while src-cache
/// stays pristine). A failed patch aborts loudly — wd is freshly
/// materialized each round so patches should apply cleanly.
pub fn apply(src: &Path, name: &str) -> Result<(), String> {
    for (i, row) in TABLE.iter().filter(|r| r.name == name).enumerate() {
        check_pin(row, name)?;
        // 补丁体落临时文件喂 patch(1)（sys::run 无 stdin 通道）/ stage the
        // patch body to a temp file for patch(1) (sys::run has no stdin)
        let tmp = src
            .parent()
            .unwrap_or(src)
            .join(format!(".ffbuild-patch-{name}-{i}.diff"));
        std::fs::write(&tmp, row.body).map_err(|e| format!("{tmp:?}: {e}"))?;
        let tmp_s = tmp.to_string_lossy().to_string();
        let src_s = src.to_string_lossy().to_string();
        let rc = sys::run(
            "patch",
            &[
                "-p1",
                "--forward",
                "-d",
                src_s.as_str(),
                "-i",
                tmp_s.as_str(),
            ],
            src,
        );
        let _ = std::fs::remove_file(&tmp);
        match rc {
            Ok(0) => crate::log::log(&format!("[patch] {name}#{i}: applied")),
            Ok(code) => return Err(format!("patch {name}#{i}: patch exited {code}")),
            Err(e) => return Err(format!("patch {name}#{i}: {e}")),
        }
    }
    Ok(())
}
