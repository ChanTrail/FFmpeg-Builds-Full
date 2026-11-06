//! leptonica 配方：图像处理（tesseract 依赖；旧 40-imaging.sh 移植）
//! leptonica recipe: image processing, tesseract dependency (port of
//! 40-imaging.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::log;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("leptonica", "linux,mac,windows", "lept", "-", body);
    x.src = include_str!("leptonica.rs");
    x
}

/// 构建 leptonica 静态库 / Build leptonica statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // 用 commit 归档而非 releases/download：后者 CDN 连返 504；
    // archive/<sha> 走 codeload 且不可变 / commit archive, not
    // releases/download: the release CDN 504'd; archive/<sha> goes through
    // codeload and is immutable
    let src = ctx.src()?;
    // SW_BUILD 默认 ON，find_package(SW) 没装 sw.org 工具时直接中止。
    // ENABLE_TIFF/JPEG=OFF：迷路的系统库会被编进去但 .pc 从不点名、前缀
    // 里也没有归档——tesseract 链接探测死于未定义。MinGW SUFFIX quirk：
    // 静态构建会产出名为 .dll 的 ar 归档。
    // SW_BUILD defaults ON and aborts without the sw.org tool. TIFF/JPEG
    // off: stray system libs get compiled in but are never pc-named and no
    // archive lands in the prefix. MinGW SUFFIX quirk: a static build emits
    // an ar archive misnamed *.dll.
    fsx::sed_i(
        "s/^if(MINGW)$/if(MINGW AND BUILD_SHARED_LIBS)/",
        &src.join("src/CMakeLists.txt"),
        &src,
    )?;
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_PROG=OFF",
            "-DSW_BUILD=OFF",
            "-DENABLE_TIFF=OFF",
            "-DENABLE_JPEG=OFF",
        ],
    )?;
    // 旧版 cmake 可能不安装 lept.pc——为 pkg-config 消费者补一份
    // older cmake versions may not install lept.pc — synthesize it for the
    // pkg-config consumers
    let pcdir = e.prefix_win.join("lib/pkgconfig");
    if !pcdir.join("lept.pc").is_file() {
        let lname = if e.prefix_win.join("lib/libleptonica.a").is_file() {
            "leptonica"
        } else {
            "lept"
        };
        fsx::write(
            &pcdir.join("lept.pc"),
            &format!(
                "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: leptonica\nDescription: leptonica image processing library\nVersion: 1.87.0\nLibs: -L${{libdir}} -l{lname}\nLibs.private: -lm\nCflags: -I${{includedir}}\n",
                e.prefix
            ),
        )?;
    }
    // 剥掉解析不到 .pc 的 Requires 记号（giflib 从不发 .pc）
    // strip Requires tokens that resolve to no .pc (giflib never ships one)
    pkgcfg::sanitize_requires(&pcdir.join("lept.pc"));
    // 导出的 cmake targets 引用 NS::name 但从不 find_dependency——改写成
    // 实际解析到的归档 / exported cmake targets reference NS::name but
    // never find_dependency — rewrite each token to the archive it
    // resolves to
    let cmdir = e.prefix_win.join("lib/cmake/leptonica");
    for f in fsx::walk(&cmdir) {
        if !f.extension().map(|x| x == "cmake").unwrap_or(false) {
            continue;
        }
        let text = fsx::read(&f);
        let mut targets: Vec<String> = Vec::new();
        for tok in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != ':') {
            if tok.matches("::").count() == 1
                && tok
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphabetic() || c == '_')
                    .unwrap_or(false)
                && !tok.ends_with("::")
            {
                targets.push(tok.to_string());
            }
        }
        targets.sort();
        targets.dedup();
        for tgt in targets {
            let n = tgt.rsplit("::").next().unwrap_or("");
            let cands: Vec<String> = match tgt.as_str() {
                "PNG::PNG" => vec!["png16".into(), "png".into()],
                "TIFF::TIFF" => vec!["tiff".into(), "tiff-6".into(), "tiff-5".into()],
                "ZLIB::ZLIB" => vec!["z".into(), "zlib".into()],
                _ => {
                    let base = n.trim_start_matches("lib");
                    vec![base.to_string(), base.to_lowercase()]
                }
            };
            let mut found = None;
            for c in &cands {
                let p = e.prefix_win.join(format!("lib/lib{c}.a"));
                if p.is_file() {
                    found = Some(p);
                    break;
                }
            }
            match found {
                Some(a) => {
                    fsx::sed_i(
                        &format!(
                            "s|{}|{}|g",
                            regex_escape(&tgt),
                            a.to_string_lossy().replace('\\', "/")
                        ),
                        &f,
                        &src,
                    )?;
                }
                None => log::warn(&format!(
                    "[extras] leptonica exports unresolved target {tgt}"
                )),
            }
        }
    }
    Ok(())
}

/// sed 模式侧转义 / escape a literal for the sed pattern side
fn regex_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('.', "\\.")
        .replace('+', "\\+")
        .replace('[', "\\[")
        .replace('*', "\\*")
}
