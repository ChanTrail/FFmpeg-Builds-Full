//! davs2 配方：AVS2 解码器
//! davs2 recipe: AVS2 decoder

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // check 带版本约束：上游 .pc 的 Version 恒为 "1.6."（三点无尾），修复
    // 若对某个平台失效（快照残留/行尾差异），约束让 .ok 校验直接失败触发
    // 重建，而不是带病缓存 / the check carries a version constraint:
    // upstream's .pc Version is always "1.6." (trailing dot, no tail), so
    // if the repair ever misses on some platform (snapshot residue / line
    // ending drift) the constraint fails .ok validation and forces a
    // rebuild instead of caching a sick .pc
    let mut x = r(
        "davs2",
        "linux:x86_64,windows:x86_64",
        "davs2 >= 1.6.0",
        "--enable-libdavs2",
        body,
    );
    x.src = include_str!("davs2.rs");
    x
}

/// 构建 davs2 / Build davs2
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let b = src.join("build/linux");
    // configure 的字节序探测在现代 binutils 下会失败；目标平台全是小端
    // the configure endianness probe fails with modern binutils;
    // everything we target is little endian
    let cf = b.join("configure");
    let text = fsx::read(&cf)
        .replace("EGIB", "bss")
        .replace("naidnePF", "bss");
    fsx::write(&cf, &text)?;
    sys::run_ok(
        "./configure",
        &[
            "--disable-cli",
            "--enable-pic",
            &format!("--prefix={}", e.prefix),
        ],
        &b,
    )?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &b)?;
    sys::run_ok("make", &["install"], &b)?;
    // 无 tag 检出时 version.sh 输出 "1.6."——非法 pkg-config 版本号，让
    // FFmpeg 的 "davs2 >= 1.6.0" 失败；行尾容忍空白（windows 检出可能
    // 带 CRLF）/ their version.sh emits "1.6." tagless — not a valid
    // pkg-config version and fails FFmpeg's check; the line tail tolerates
    // whitespace (a windows checkout may carry CRLF endings)
    let pc = e.prefix_win.join("lib/pkgconfig/davs2.pc");
    fsx::sed_i(
        "s/^\\(Version:[[:space:]]*[0-9.]\\+\\)\\.[[:space:]]*$/\\1.0/",
        &pc,
        &e.work,
    )?;
    // 修后自检：版本行若仍以裸点结尾就当场炸掉——比带病 .ok 无限续命强，
    // 失败信息里还能留下真实的 .pc 内容供排查 / post-fix assertion: if
    // the version line still ends in a bare dot, fail right here — beats
    // a sick .ok living forever, and the error message preserves the
    // actual .pc content for diagnosis
    let fixed = fsx::read(&pc);
    for l in fixed.lines() {
        let l = l.trim_end();
        if l.starts_with("Version") && l.ends_with('.') {
            return Err(format!("davs2.pc version still malformed:\n{fixed}"));
        }
    }
    Ok(())
}
