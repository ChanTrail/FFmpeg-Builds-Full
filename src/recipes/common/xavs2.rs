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
    // 与 davs2 同款 version.sh 怪癖：无 tag 检出产生 "1.7."；is_file 守卫
    // 与 let _ 吞错已在实证中让坏 .pc 带病缓存，拆掉——缺文件或修补失败
    // 都要当场炸；行尾容忍空白 / same version.sh quirk as davs2: a
    // tagless checkout yields "1.7."; the is_file guard and `let _ =`
    // error swallow demonstrably let a sick .pc get cached — removed:
    // a missing file or a failed patch must blow up right here; the line
    // tail tolerates whitespace
    let pc = e.prefix_win.join("lib/pkgconfig/xavs2.pc");
    fsx::sed_i(
        "s/^\\(Version:[[:space:]]*[0-9.]\\+\\)\\.[[:space:]]*$/\\1.0/",
        &pc,
        &e.work,
    )?;
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
