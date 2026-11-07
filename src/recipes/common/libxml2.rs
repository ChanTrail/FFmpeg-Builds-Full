//! libxml2 配方：XML 解析库（macOS 只有 SDK 动态桩，静态归档要自造）
//! libxml2 recipe: the XML parser (macOS ships only an SDK dylib stub —
//! the static archive has to be built here)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libxml2",
        "linux,mac",
        "libxml-2.0",
        "--enable-libxml2",
        body,
    );
    x.src = include_str!("libxml2.rs");
    x
}

/// 构建 libxml2 / Build libxml2
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // macOS SDK 只给 libxml2.tbd 动态桩——没有 .pc、没有归档。前缀缺
    // libxml-2.0 时死的是两条链：
    // * librsvg：meson 的 dependency(['libxml-2.0','LibXml2']) 落到
    //   CMake FindLibXml2 兜底命中 .tbd，meson.build 随即给 cargo 导出
    //   SYSTEM_DEPS_LIBXML2_{NO_PKG_CONFIG,SEARCH_NATIVE,LIB}——其中
    //   LIB 取 .tbd 的词干 "libxml2"（"lib" 前缀被算进名字本体），再
    //   叠上静态构建的 SYSTEM_DEPS_LINK=static，cargo 输出
    //   rustc-link-lib=static=libxml2 → rustc 去搜永不存在的
    //   liblibxml2.a："could not find native static library `libxml2`"
    // * FFmpeg：libxml-2.0 探测同样落空，--enable-libxml2 被静默降级
    //   成 --disable-libxml2（DASH 清单的 XML 解析整个丢掉）
    // 前缀里装进真归档后两条链都回到 pkg-config 正道：meson 第一路即
    // 中、不再导覆盖变量；cargo 端 static=xml2 在 <prefix>/lib 命中
    // libxml2.a；FFmpeg 端 --enable-libxml2 真正生效。
    // The macOS SDK ships only the libxml2.tbd stub — no .pc, no
    // archive. With libxml-2.0 missing from the prefix, two chains die:
    // * librsvg: meson's dependency(['libxml-2.0','LibXml2']) falls back
    //   to CMake FindLibXml2 and hits the .tbd; meson.build then exports
    //   SYSTEM_DEPS_LIBXML2_{NO_PKG_CONFIG,SEARCH_NATIVE,LIB} for cargo
    //   — LIB is the .tbd stem "libxml2" (the "lib" prefix counted as
    //   part of the name) — and the static build adds
    //   SYSTEM_DEPS_LINK=static, so cargo emits
    //   rustc-link-lib=static=libxml2 → rustc searches for liblibxml2.a,
    //   which can never exist: "could not find native static library
    //   `libxml2`"
    // * FFmpeg: the libxml-2.0 probe misses identically and
    //   --enable-libxml2 silently degrades to --disable-libxml2 (DASH
    //   manifest XML parsing lost entirely)
    // Installing a real archive into the prefix puts both chains back on
    // the pkg-config path: meson's first lookup succeeds and exports no
    // overrides; cargo's static=xml2 finds libxml2.a in <prefix>/lib;
    // FFmpeg's --enable-libxml2 takes effect for real.
    buildsys::meson_build(
        &src,
        &[
            // iconv 保留：macOS SDK 自带 -liconv，FFmpeg 的 --extra-libs
            // 本来也带它 / keep iconv: the macOS SDK has -liconv and
            // FFmpeg's --extra-libs already carries it
            "-Diconv=enabled",
            // 其余全减：python/readline/history/docs/modules/zlib 都不
            // 需要——少一个依赖就少一条静态解析链（lzma 在 libxml2
            // master 上已没有同名 meson 选项，不能传）
            // everything else subtracted: python/readline/history/docs/
            // modules/zlib aren't needed — one less dependency is one
            // less static-resolution chain (lzma has no same-named meson
            // option on libxml2 master, so it must not be passed)
            "-Dpython=disabled",
            "-Dreadline=disabled",
            "-Dhistory=disabled",
            "-Ddocs=disabled",
            "-Dmodules=disabled",
            "-Dzlib=disabled",
            "--wrap-mode=nofallback",
        ],
    )
}
