//! xavs2 配方：AVS2 编码器
//! xavs2 recipe: AVS2 encoder

use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // check 带版本约束：与 davs2 同理，修复失效时约束让 .ok 校验失败
    // 触发重建而不是带病缓存 / the check carries a version constraint:
    // same reasoning as davs2 — if the repair ever misses, the constraint
    // fails .ok validation and forces a rebuild instead of caching a sick
    // .pc
    let mut x = r(
        "xavs2",
        "linux:x86_64,windows:x86_64",
        "xavs2 >= 1.3.0",
        "--enable-libxavs2",
        body,
    );
    x.src = include_str!("xavs2.rs");
    x
}

/// 构建 xavs2 / Build xavs2
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let b = src.join("build/linux");
    let cf = b.join("configure");
    let text = fsx::read(&cf)
        .replace("EGIB", "bss")
        .replace("naidnePF", "bss");
    fsx::write(&cf, &text)?;
    // SIGSEGV 真凶已定位（run 43178495100 + 43236158220 双取证）：不
    // 是 asm、不是 WPP 行池——是栈对齐。configure 见 GNU 就塞
    // -mpreferred-stack-boundary=5（~line 866），GCC 由此把"调用方
    // rsp 已 32B 对齐"当成前提，给 ALIGN32 局部数组发 vmovdqa 且不
    // 重对齐 rsp；ABI 只保 16B。ffmpeg 侧（标准 16B ABI）进入
    // xavs2 后，沿途不需重对齐的帧把奇偶性原样传递——进 dct_32x32_c
    // 时 rsp≡16 (mod 32) 就 vmovdqa → #GP → SIGSEGV。逐路径定死、
    // 改代码就变，解释了历次"leg 漂移"。objdump 实证：崩指令正是
    // 函数 +0x68 处写 block[] 的 vmovdqa %ymm0,-0x40(%rax)。
    // -mincoming-stack-boundary=4 把入栈假设拉回 ABI 真值（16B），
    // GCC 对需要 32B 的函数补 and $-32,%rsp——确定性修复；preferred
    // =5 保留，对齐路径照走。串行 WPP 补丁已证伪撤销（串行照样崩）
    // / the SIGSEGV is now root-caused (runs 43178495100+43236158220):
    // not asm, not the WPP row pool — stack alignment. configure feeds
    // -mpreferred-stack-boundary=5 to GCC (~line 866), which then
    // assumes callers hand over a 32B-aligned rsp and emits vmovdqa on
    // ALIGN32 locals with no rsp realignment — while the ABI promises
    // only 16B. Frames along the ffmpeg→xavs2 chain that don't need
    // realignment propagate incoming parity unchanged, so dct_32x32_c
    // inherits whatever the path produced — parity ≡16 (mod 32) makes
    // its vmovdqa #GP. Path-deterministic and code-shift-sensitive,
    // which explains the historic "migrating leg" crashes. objdump
    // proved it: the faulting instruction is the vmovdqa
    // %ymm0,-0x40(%rax) store into block[] at +0x68.
    // -mincoming-stack-boundary=4 restores the incoming assumption to
    // the ABI truth (16B); GCC then emits and $-32,%rsp wherever a
    // function needs 32B — a deterministic fix. preferred=5 stays, so
    // aligned paths still get their fast stores. The serial-WPP patch
    // was falsified and reverted (serial crashed too)
    // --disable-thread 依然不可走——上游该路全线 bitrot（osdep.h 只桩
    // 3/15 个线程 shim，threadpool.c 无 HAVE_THREAD 守卫，AEC/REC
    // 架构长在 worker 上），Windows 下还拆 windows.h include 链 /
    // --disable-thread is still a dead end — upstream bitrotted it
    // everywhere (osdep.h stubs only 3/15 thread shims, threadpool.c
    // has no HAVE_THREAD guard, the AEC/REC architecture lives on
    // workers), and on Windows it also removes the windows.h include
    // chain (xavs2_mdate fails to compile)
    let args = [
        "--disable-cli",
        "--enable-static",
        "--enable-pic",
        "--disable-asm",
        "--disable-avs",
        "--disable-swscale",
        "--disable-lavf",
        "--disable-ffms",
        "--disable-gpac",
        "--disable-lsmash",
        "--extra-asflags=-w-macro-params-legacy",
        "--extra-cflags=-Wno-error=incompatible-pointer-types",
        "--extra-cflags=-mincoming-stack-boundary=4",
    ];
    let prefix = format!("--prefix={}", e.prefix);
    let refs: Vec<&str> = args.iter().copied().chain([prefix.as_str()]).collect();
    sys::run_ok("./configure", &refs, &b)?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &b)?;
    sys::run_ok("make", &["install"], &b)?;
    // 与 davs2 同款 version.sh 怪癖：无 tag 检出产生 "1.3."。直接在
    // Rust 里改：实证 runner 上外部 sed 静默不匹配，读写均可见且行尾
    // 原样保留 / same version.sh quirk as davs2: a tagless checkout
    // yields "1.3.". Edit in Rust directly: the external sed silently
    // mismatched on the runner; the rewrite is fully visible here and
    // line endings are preserved verbatim
    let pc = e.prefix_win.join("lib/pkgconfig/xavs2.pc");
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
    // 修后自检：与 davs2 同理，版本行仍以裸点结尾就当场炸 / post-fix
    // assertion, same as davs2: a version line still ending in a bare dot
    // fails right here
    let fixed = fsx::read(&pc);
    for l in fixed.lines() {
        let l = l.trim_end();
        if l.starts_with("Version") && l.ends_with('.') {
            return Err(format!("xavs2.pc version still malformed:\n{fixed}"));
        }
    }
    Ok(())
}
