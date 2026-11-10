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
    sys::run_ok(
        "./configure",
        &[
            "--disable-cli",
            "--enable-static",
            "--enable-pic",
            // asm nasm de 2019 + pool interna do xavs2 são as superfícies
            // do crash silencioso (~0.5s, exit -1/sinal) visto nas legs
            // linux/win x86_64; -threads 1 do ffmpeg não controla a pool
            // interna. C puro garante execução — encoder é só cobertura /
            // 2019-era nasm asm + xavs2's own threadpool are the silent
            // crash surfaces (~0.5s signal death on linux/win x86_64 legs);
            // ffmpeg's -threads 1 does not control xavs2's internal pool.
            // Pure C guarantees execution — this encoder is for coverage
            "--disable-asm",
            "--disable-thread",
            "--disable-avs",
            "--disable-swscale",
            "--disable-lavf",
            "--disable-ffms",
            "--disable-gpac",
            "--disable-lsmash",
            "--extra-asflags=-w-macro-params-legacy",
            "--extra-cflags=-Wno-error=incompatible-pointer-types",
            &format!("--prefix={}", e.prefix),
        ],
        &b,
    )?;
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
