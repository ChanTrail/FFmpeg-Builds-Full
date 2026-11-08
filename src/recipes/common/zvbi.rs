//! zvbi 配方：图文电视/VBI 解码
//! zvbi recipe: teletext/VBI decoding

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("zvbi", "linux,mac", "zvbi-0.2", "--enable-libzvbi", body);
    x.src = include_str!("zvbi.rs");
    x
}

/// 构建 libzvbi / Build libzvbi
///
/// FFmpeg 只吃 libzvbi.a——proxy 守护进程、tests、examples、contrib 工具
/// 一律不装；它们都要拿 libzvbi.a 链程序，而上游把 -lpng 追加在 -lm 之后
/// （LIBS="-lpthread -lm -lpng -lz"），静态 libpng 的 pow 引用在已过扫描的
/// -lm 后面解不开（linux-x86_64 实测 x11font 阵亡）。contrib 没有开关，由
/// patches/zvbi/0001-drop-contrib-subdir.patch 从 SUBDIRS 摘掉；其余目录
/// 走下面三个 --disable 旗标。
/// FFmpeg only links libzvbi.a — the proxy daemon, tests, examples and
/// contrib tools are never installed; each links programs against
/// libzvbi.a, and upstream appends -lpng after -lm (LIBS="-lpthread -lm
/// -lpng -lz"), so a static libpng's pow reference can't resolve behind an
/// already-scanned -lm (observed: x11font dies on linux-x86_64). contrib
/// has no switch and is dropped from SUBDIRS by
/// patches/zvbi/0001-drop-contrib-subdir.patch; the rest are covered by
/// the three --disable flags below.
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::autotools_build(
        &src,
        &[
            "--disable-dvb",
            "--disable-bktr",
            "--disable-nls",
            "--without-doxygen",
            "--disable-proxy",
            "--disable-tests",
            "--disable-examples",
        ],
    )
}
