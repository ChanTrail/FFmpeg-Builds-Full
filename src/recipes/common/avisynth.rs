//! avisynth 配方：AviSynth+ 头文件（仅头文件——FFmpeg 运行时才加载本
//! 体）
//! avisynth recipe: AviSynth+ headers (header-only — FFmpeg loads the
//! body at run time)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "avisynth",
        "all",
        "hdrsym:avisynth/avisynth_c.h@avs_get_channel_mask",
        "--enable-avisynth",
        body,
    );
    x.src = include_str!("avisynth.rs");
    x
}

/// 安装 AviSynth+ 头文件 / Install the AviSynth+ headers
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // .git 存在不等于有效：copy_dir 只收文件，全空的 refs/ 层整层蒸发，
    // 剩 HEAD+objects+packed-refs 照样 "not a git repository"。以
    // rev-parse 探活为准——死了剥壳重建。探针钉 .git 本体：裸探会沿父目
    // 录发现外层 checkout（工作根在仓库内），把死壳当活库 / an existing
    // .git is not necessarily valid: copy_dir carries files only, so an
    // all-empty refs/ layer vanishes wholesale and HEAD+objects+
    // packed-refs still reads "not a git repository". Trust a rev-parse
    // liveness probe — strip and re-init when it dies. The probe pins
    // .git itself: a bare probe walks up into the enclosing checkout
    // (work root sits inside the repo) and passes the corpse off as live
    if sys::run("git", &["--git-dir=.git", "rev-parse", "--git-dir"], &src) != Ok(0) {
        crate::fsx::rm_rf(&src.join(".git"));
        sys::run_ok("git", &["init", "-q"], &src)?;
    }
    // 至少一个 commit 给 tag/describe——新 init 的空库需要，带历史的真
    // 检出直接过 / at least one commit for tag/describe — needed by a
    // freshly init'd empty repo, free when a real checkout has history
    if sys::run("git", &["rev-parse", "--verify", "HEAD"], &src) != Ok(0) {
        sys::run_ok(
            "git",
            &[
                "-c",
                "user.name=ci",
                "-c",
                "user.email=ci@localhost",
                "commit",
                "--allow-empty",
                "-m",
                "ffbuild",
            ],
            &src,
        )?;
    }
    // 它的版本检查坚持要求存在 tag / their version check insists on a tag
    let _ = sys::run(
        "git",
        &[
            "-c",
            "user.name=ci",
            "-c",
            "user.email=ci@localhost",
            "tag",
            "-a",
            "ffbuild",
            "-m",
            "ffbuild",
        ],
        &src,
    );
    sys::run_ok(
        "cmake",
        &[
            "-S",
            ".",
            "-B",
            "build",
            "-DCMAKE_BUILD_TYPE=Release",
            &format!("-DCMAKE_INSTALL_PREFIX={}", e.prefix),
            "-DHEADERS_ONLY=ON",
            "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
        ],
        &src,
    )?;
    // `cmake --install` 是 3.15+ 子命令；生成器自己的 install 目标任何
    // 版本都认 / `cmake --install` needs 3.15+; the generator's own
    // install target works on any vintage
    sys::run_ok(
        "cmake",
        &["--build", "build", "--target", "VersionGen"],
        &src,
    )?;
    sys::run_ok("cmake", &["--build", "build", "--target", "install"], &src)
}
