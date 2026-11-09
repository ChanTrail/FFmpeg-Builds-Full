//! 冒烟测试：对解压后的 FFmpeg 构建做体检
//! Smoke test: sanity-check an extracted FFmpeg build
//! 在每个发布的平台上运行，只依赖 ffmpeg/ffprobe 二进制本身。
//! Runs on every shipped platform; relies only on the ffmpeg/ffprobe
//! binaries themselves.

use crate::log;
use crate::sys;
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

/// 每个发布构建都必须有的编码器 / encoders every shipped build must have
const REQUIRED_ENCODERS: &[&str] = &["libx264", "libopus", "libmp3lame"];
/// 存在于此构建时就真实试编码的编码器 / encoders exercised for real when
/// present in this build
const TRIED_ENCODERS: &[&str] = &[
    "libx264",
    "libx265",
    "libvpx-vp9",
    "libsvtav1",
    "libaom-av1",
];

/// smoke_test <bin-dir>：版本/许可证/编码器清单/真实编码体检
/// Version/license/encoder-inventory/real-encode checks
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

    // --- 哪些外部库进了构建 / which external libraries made it in ---
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

    // --- 必需编码器 / required encoders ---
    let encoders = sys::capture(&fm, &["-hide_banner", "-encoders"], bin)
        .map_err(|e| format!("ffmpeg -encoders: {e}"))?;
    let has = |enc: &str| encoders.split_whitespace().any(|t| t == enc);
    for enc in REQUIRED_ENCODERS {
        if !has(enc) {
            return fail(&format!("required encoder '{enc}' is missing"));
        }
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
            "sine=frequency=440:duration=1",
            "-c:a",
            "libmp3lame",
            "-f",
            "null",
            "-",
        ],
        bin,
    )? != 0
    {
        return fail("libmp3lame encode failed");
    }
    log::group_end();

    // 把每个编码器都试一遍再放弃：一次 run 报出所有坏掉的
    // try every encoder before giving up so one run reports all broken ones
    let mut failed: Vec<String> = Vec::new();
    for enc in TRIED_ENCODERS {
        if !has(enc) {
            println!("encode test: {enc} not present in this build, skipped");
            continue;
        }
        println!("encode test: {enc}");
        let args = [
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
        ];
        let rc = sys::run(&fm, &args, bin).unwrap_or(1);
        if rc == 0 {
            continue;
        }
        println!("encode test: {enc} FAILED (exit status {rc})");
        // SIMD 掩码诊断：libaom 读 AOM_SIMD_CAPS_MASK——0 掩掉全部 SIMD
        // 档回落纯 C，过即"SIMD 路径崩"反证（libaom-av1 在 avx512 平台
        // 的 0xC0000005 实证）。其它编码器不吃这个变量，无副作用 /
        // SIMD-mask diagnostic: libaom honors AOM_SIMD_CAPS_MASK — 0
        // masks every SIMD level onto the C path, so a pass is
        // counter-evidence for a SIMD-path crash (empirical:
        // libaom-av1's 0xC0000005 on an avx512 host). Other encoders
        // ignore the var — no side effects
        let drc = sys::cmd(&fm, &args)
            .env("AOM_SIMD_CAPS_MASK", "0")
            .status()
            .map(|s| s.code().unwrap_or(-1))
            .unwrap_or(1);
        println!("encode test: {enc} retried with AOM_SIMD_CAPS_MASK=0 -> exit {drc}");
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
            let target = std::env::var("FFMPEG_GDB").unwrap_or_else(|_| fm.clone());
            let _ = sys::run(
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
                    "bt 30",
                    "-ex",
                    "thread apply all bt 30",
                    "-ex",
                    "info registers",
                    "-ex",
                    "info sharedlibrary",
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
            );
            log::group_end();
        }
        annotate_error(&format!(
            "encoder '{enc}' is present but failed to encode (exit status {rc})"
        ));
        failed.push(enc.to_string());
    }
    if !failed.is_empty() {
        return Err(format!(
            "smoke test failed: encoders that are present but cannot encode: {}",
            failed.join(" ")
        ));
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
