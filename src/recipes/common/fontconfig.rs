//! fontconfig 配方：字体枚举/匹配库（macOS 无系统 fontconfig，静态
//! 归档要自造）
//! fontconfig recipe: font enumeration/matching library (macOS has no
//! system fontconfig — the static archive has to be built here)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "fontconfig",
        "linux,mac",
        "fontconfig",
        "--enable-libfontconfig",
        body,
    );
    x.src = include_str!("fontconfig.rs");
    x
}

/// 构建 fontconfig / Build fontconfig
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 前缀缺 fontconfig 时 pango 的 meson 自动探测跳过 fontconfig 后端
    // → 不产 pangoft2 → librsvg 的 rsvg-convert 无条件 extern
    // `#[cfg(unix)] FcGetVersion`（上游假定 unix 必有 fontconfig；
    // pangoft2_dep 在 darwin 仅 optional）在最终链接报
    // Undefined symbol。装进前缀后链全通：
    //   fontconfig.pc → pango 建 pangoft2 → pangoft2.pc 的
    //   Libs.private 带 -lfontconfig -lfreetype → cargo 链接解析
    //   FcGetVersion；顺带 FFmpeg 的 --enable-libfontconfig 探测
    //   （features.rs:124 exists("fontconfig")）也不再静默降级。
    // without fontconfig in the prefix, pango's meson auto-detect skips
    // the fontconfig backend → no pangoft2 → librsvg's rsvg-convert,
    // which unconditionally externs `#[cfg(unix)] FcGetVersion`
    // (upstream assumes every unix has fontconfig; pangoft2_dep is only
    // optional on darwin), dies at link with an undefined symbol.
    // Installed into the prefix the whole chain links through:
    //   fontconfig.pc → pango builds pangoft2 → pangoft2.pc's
    //   Libs.private carries -lfontconfig -lfreetype → cargo resolves
    //   FcGetVersion; FFmpeg's --enable-libfontconfig probe
    //   (features.rs:124 exists("fontconfig")) also stops degrading.
    //
    // -Dxml-backend=libxml2：auto 会先探 expat——命中 SDK 的
    // libexpat.tbd 就把一份系统动态库依赖缝进静态链；显式用前缀自造
    // 的静态 libxml2 / auto probes expat first — hitting the SDK's
    // libexpat.tbd would sew a system dylib dependency into the static
    // chain; pin the prefix's own static libxml2 instead
    buildsys::meson_build(
        &src,
        &[
            "-Ddoc=disabled",
            "-Dtests=disabled",
            "-Dtools=disabled",
            "-Dcache-build=disabled",
            "-Dnls=disabled",
            "-Dxml-backend=libxml2",
            "--wrap-mode=nofallback",
        ],
    )
}
