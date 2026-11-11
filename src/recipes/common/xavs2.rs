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
    // asm nasm de 2019 与内部 pool 是静默 SIGSEGV（~0.5s，-11）的两
    // 个嫌疑面：crash 跨 leg 漂移 = VM CPU 不同 → asm dispatch；4
    // worker 线程同构挂起 + 主线程崩在派发路径 → pool。asm 全平台
    // 关；thread 仅 Linux 关——Windows 下 --disable-thread 拆掉
    // windows.h 的 include 链（xavs2_mdate 编译炸，实证），且
    // Windows 腿带 pool 通过 / 2019 nasm asm and the internal pool
    // are the two silent-SIGSEGV suspects (~0.5s, -11): leg-migrating
    // crash = per-VM CPU asm dispatch; 4 identically-parked worker
    // threads + main dead inside dispatch = pool. asm off everywhere;
    // thread off on Linux only — on Windows it removes the windows.h
    // include chain (xavs2_mdate fails, proven) and the Windows leg
    // passes with the pool anyway
    let mut args = vec![
        "--disable-cli".to_string(),
        "--enable-static".into(),
        "--enable-pic".into(),
        "--disable-asm".into(),
    ];
    // Linux 关线程要补 sched.h：cpu.c:358 有一条手写的孤儿
    // sched_getaffinity 声明（SYS_LINUX 守卫不带 HAVE_CPU_COUNT），
    // thread=no 时 sched.h 不进场 → cpu_set_t 未声明编译炸；函数体
    // 内真正的调用点都在 !HAVE_THREAD 早退的死支里，类型补上即过 /
    // Linux's --disable-thread needs sched.h injected: cpu.c:358 has
    // an orphaned handwritten sched_getaffinity declaration (the
    // SYS_LINUX guard lacks HAVE_CPU_COUNT), and with thread=no
    // sched.h never arrives → undeclared cpu_set_t breaks the build;
    // the real call sites sit in the !HAVE_THREAD early-return dead
    // branch — supplying the type is enough
    let cflags = if e.os == sys::Os::Linux {
        args.push("--disable-thread".into());
        "-Wno-error=incompatible-pointer-types -include sched.h"
    } else {
        "-Wno-error=incompatible-pointer-types"
    };
    args.extend(
        [
            "--disable-avs",
            "--disable-swscale",
            "--disable-lavf",
            "--disable-ffms",
            "--disable-gpac",
            "--disable-lsmash",
            "--extra-asflags=-w-macro-params-legacy",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    args.push(format!("--extra-cflags={cflags}"));
    args.push(format!("--prefix={}", e.prefix));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
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
