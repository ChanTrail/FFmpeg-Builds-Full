//! vapoursynth 配方：帧服务端/插件宿主——核心绑定要 cython 生成，venv
//! 隔离绕开 PEP-668
//! vapoursynth recipe: the frameserver/plugin host — its core bindings
//! need cython; a venv sidesteps PEP-668

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "vapoursynth",
        "linux,mac",
        "vapoursynth",
        "--enable-vapoursynth",
        body,
    );
    x.src = include_str!("vapoursynth.rs");
    x
}

/// 构建 VapourSynth / Build VapourSynth
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // cython 生成 src/cython 绑定——系统 python 受 PEP-668 保护不让
    // pip install，进 venv / cython generates the src/cython bindings —
    // system python is PEP-668-managed and refuses pip installs, so use
    // a venv
    let venv = ctx.wd.join("venv");
    sys::run_ok("python3", &["-m", "venv", &venv.to_string_lossy()], &ctx.wd)?;
    sys::run_ok(
        &format!("{}/bin/pip", venv.display()),
        &["install", "cython"],
        &ctx.wd,
    )?;
    // 子进程 PATH 是 sys::cmd 从 env.child_path 定死的——EnvGuard 改
    // 进程内 PATH 传不下去；child_path 含 <prefix>/bin，把 cython 脚本
    // 软链进去（shebang 指向 venv python，挪窝照跑）/ child PATH is
    // fixed by sys::cmd from env.child_path — an EnvGuard PATH tweak
    // never reaches the child; child_path does include <prefix>/bin so
    // symlink the cython script there (its shebang points at the venv
    // python and runs fine from anywhere)
    let bindir = e.prefix_win.join("bin");
    crate::fsx::install_d(&bindir)?;
    sys::run_ok(
        "ln",
        &[
            "-sf",
            &venv.join("bin/cython").to_string_lossy(),
            &bindir.join("cython").to_string_lossy(),
        ],
        &ctx.wd,
    )?;
    buildsys::meson_build(&src, &[])
}
