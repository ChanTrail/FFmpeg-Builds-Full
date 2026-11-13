//! opus 配方：Opus 音频编解码
//! opus recipe: the Opus audio codec

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("opus", "linux,mac", "opus", "--enable-libopus", body);
    x.src = include_str!("opus.rs");
    x
}

/// 构建 opus / Build opus
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // git 树里 dnn/fargan_data.{c,h} 等 *_data 生成文件并不入库（fargan
    // 也不是 submodule）——lpcnet_*.mk 无条件列出它们，meson 的 files()
    // 缺一个就 error。autogen.sh 里钉着模型包 sha256：
    //   dnn/download_model.sh "<sha>" → 拉 opus_data-<sha>.tar.gz 解开，
    //   生成文件全部落到 dnn/ 下
    // the git tree does not check in the generated dnn/*_data.{c,h}
    // files (fargan isn't a submodule either) — lpcnet_*.mk lists them
    // unconditionally and meson's files() errors on any miss. autogen.sh
    // pins the model tarball sha256:
    //   dnn/download_model.sh "<sha>" fetches opus_data-<sha>.tar.gz and
    //   unpacks every generated file under dnn/
    let ag = fsx::read(&src.join("autogen.sh"));
    let sha = ag
        .split("download_model.sh")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .map(str::to_string)
        .ok_or("opus: model sha256 not found in autogen.sh")?;
    sys::run_ok("sh", &["dnn/download_model.sh", &sha], &src)?;
    buildsys::meson_build(
        &src,
        &[
            "-Ddocs=disabled",
            "-Dtests=disabled",
            "-Dextra-programs=disabled",
        ],
    )
}
