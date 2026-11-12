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
    // 与 xavs2 同源 configure：见 GNU 就塞 -mpreferred-stack-boundary=5，
    // GCC 由此假设入栈已 32B 对齐、给 ALIGN32 局部发 vmovdqa 且不重对齐
    // rsp——ABI 只保 16B，同款的"按调用路径掷硬币 SIGSEGV"隐患。
    // -mincoming-stack-boundary=4 拉回真值，需要 32B 的函数补 and $-32 /
    // same configure DNA as xavs2: it feeds -mpreferred-stack-boundary=5
    // to GCC, which then assumes a 32B-aligned incoming rsp and emits
    // vmovdqa on ALIGN32 locals with no realignment — the ABI only
    // promises 16B, the same call-path-coin-flip SIGSEGV latent defect.
    // -mincoming-stack-boundary=4 restores the true assumption so GCC
    // inserts and $-32,%rsp wherever 32B is needed
    sys::run_ok(
        "./configure",
        &[
            "--disable-cli",
            "--enable-pic",
            "--extra-cflags=-mincoming-stack-boundary=4",
            &format!("--prefix={}", e.prefix),
        ],
        &b,
    )?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &b)?;
    sys::run_ok("make", &["install"], &b)?;
    // 无 tag 检出时 version.sh 输出 "1.6."——非法 pkg-config 版本号，让
    // FFmpeg 的 "davs2 >= 1.6.0" 失败。直接在 Rust 里改：实证 runner 上
    // 外部 sed 静默不匹配（方言/PATH 解析不再排查），读写均可见且行尾
    // 原样保留 / their version.sh emits "1.6." tagless — not a valid
    // pkg-config version and fails FFmpeg's check. Edit in Rust directly:
    // the external sed silently mismatched on the runner (dialect/PATH
    // resolution no longer worth diagnosing); the rewrite is fully
    // visible here and line endings are preserved verbatim
    let pc = e.prefix_win.join("lib/pkgconfig/davs2.pc");
    let text = fsx::read(&pc);
    let mut out = String::with_capacity(text.len() + 4);
    for l in text.split_inclusive('\n') {
        let t = l.trim_end();
        if t.starts_with("Version") && t.ends_with('.') {
            // 截到去掉行尾的行本体，补 "0" 后还原原行尾 / take the line
            // body without its ending, append "0", then restore the
            // original ending
            let eol = &l[t.len()..];
            out.push_str(t);
            out.push('0');
            out.push_str(eol);
        } else {
            out.push_str(l);
        }
    }
    fsx::write(&pc, &out)?;
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
