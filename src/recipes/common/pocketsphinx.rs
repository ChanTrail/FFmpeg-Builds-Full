//! pocketsphinx 配方：语音识别
//! pocketsphinx recipe: speech recognition

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "pocketsphinx",
        "all",
        "pocketsphinx",
        "--enable-pocketsphinx",
        body,
    );
    x.src = include_str!("pocketsphinx.rs");
    x
}

/// 构建 pocketsphinx / Build pocketsphinx
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 默认声学模型目录是运行时查找：MODELDIR 编译期常量默认
    // CMAKE_INSTALL_FULL_DATADIR/pocketsphinx/model → 烤
    // /opt/ffbuild/share/pocketsphinx/model，ffmpeg asr 滤镜不显式传
    // hmm/lm/dict 时按它找 en-us 模型——用户机上没有即死。指系统
    // 标准位：Linux /usr/share/pocketsphinx/model（发行版
    // pocketsphinx-en-us 包），macOS 随 Homebrew /opt/homebrew/share
    // （上游 set(MODELDIR ...) 是普通变量会盖命令行 -DMODELDIR，改
    // DATADIR 才是有效入口）；DATADIR 兼安装目录，经 destdir stage。
    // Windows 原样 / the default acoustic-model dir is a runtime
    // lookup: the MODELDIR compile-time constant defaults to
    // CMAKE_INSTALL_FULL_DATADIR/pocketsphinx/model → baked
    // /opt/ffbuild/share/pocketsphinx/model — ffmpeg's asr filter
    // resolves the en-us model by it when hmm/lm/dict aren't passed,
    // dying on user machines. Point at system-standard locations:
    // /usr/share/pocketsphinx/model on Linux (distro
    // pocketsphinx-en-us package), /opt/homebrew/share on macOS per
    // Homebrew (upstream's plain set(MODELDIR ...) shadows a -D
    // override — DATADIR is the effective entry point); DATADIR
    // doubles as the install dir — stage via destdir. Windows keeps
    // the default
    let mut args = vec!["-DBUILD_TESTING=OFF"];
    let datadir: String;
    if e.os == crate::sys::Os::Mac {
        datadir = "-DCMAKE_INSTALL_DATADIR=/opt/homebrew/share".into();
        args.push(datadir.as_str());
    } else if e.os == crate::sys::Os::Linux {
        datadir = "-DCMAKE_INSTALL_DATADIR=/usr/share".into();
        args.push(datadir.as_str());
    }
    if e.os == crate::sys::Os::Windows {
        buildsys::cmake_build(&src, &args)?;
    } else {
        buildsys::cmake_build_destdir(&src, &args, &src.join("psstage"))?;
    }
    // 上游把主头文件平铺装成 include/pocketsphinx.h；FFmpeg 引用的是
    // <pocketsphinx/pocketsphinx.h>——桥接布局 / upstream installs the main
    // header flat; FFmpeg includes <pocketsphinx/pocketsphinx.h> — bridge
    let flat = e.prefix_win.join("include/pocketsphinx.h");
    let nested = e.prefix_win.join("include/pocketsphinx/pocketsphinx.h");
    if flat.is_file() && !nested.is_file() {
        fsx::install(&flat, &nested)?;
    }
    Ok(())
}
