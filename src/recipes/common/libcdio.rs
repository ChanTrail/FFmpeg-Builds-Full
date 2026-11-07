//! libcdio 配方：CD-ROM 访问库
//! libcdio recipe: CD-ROM access library

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys::{self, Os};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("libcdio", "linux,mac", "libcdio", "-", body);
    x.src = include_str!("libcdio.rs");
    x
}

/// 构建 libcdio / Build libcdio
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // 2.2.x+ 只在 GitHub 发布；tag tarball 不带 configure——走
    // autotools_build 的 autoreconf 路径 / 2.2.x+ is GitHub-only; the tag
    // tarball ships no configure — autoreconf path covers it
    let src = ctx.src()?;
    // autogen.sh 末尾是 ./configure --enable-maintainer-mode——autotools_build
    // 优先跑它且不带参数，help2man 变成硬依赖，我们的旗标永远轮不到出场。
    // 删掉走 autoreconf 路径 / autogen.sh ends with
    // `./configure --enable-maintainer-mode` — autotools_build prefers it
    // with no args, so help2man becomes mandatory and our flags never reach
    // configure. Remove it so the autoreconf path runs.
    fsx::rm_rf(&src.join("autogen.sh"));
    // git 归档不带生成的 version.texi/libcdio.info——doc 的 make 会调
    // makeinfo。touch 桩文件让目标比源新，整条 texinfo 链路被跳过
    // the archive ships no version.texi/libcdio.info and doc's make calls
    // makeinfo; touching stubs skips the whole texinfo chain
    if let Some(texi) = fsx::walk(&src)
        .into_iter()
        .find(|f| f.file_name().map(|n| n == "libcdio.texi").unwrap_or(false))
        && let Some(doc) = texi.parent()
    {
        let _ = std::fs::write(doc.join("version.texi"), "");
        let _ = std::fs::write(doc.join("libcdio.info"), "");
    }
    // macOS：libSystem 自带 BSD iconv 够用，但 AM_ICONV 的 run-test 判
    // "not working"→HAVE_ICONV 未定义→utf8.c 编译期 #error。缓存变量判
    // yes 跳过 run-test / macOS: libSystem's BSD iconv suffices but
    // AM_ICONV's run test says "not working" → HAVE_ICONV undefined →
    // utf8.c #error. Force the cache var.
    let _g = if e.os == Os::Mac {
        Some(sys::EnvGuard::set(&[(
            "am_cv_func_iconv_works",
            "yes".to_string(),
        )]))
    } else {
        None
    };
    buildsys::autotools_build(
        &src,
        &[
            "--disable-maintainer-mode",
            "--disable-cxx",
            "--disable-cddb",
            "--disable-example-progs",
            "--disable-vcd-info",
            "--without-cd-drive",
            "--without-cd-info",
            "--without-cdda-player",
            "--without-cd-read",
            "--without-iso-info",
            "--without-iso-read",
        ],
    )
}
