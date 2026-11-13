//! tesseract 配方：OCR 库
//! tesseract recipe: OCR library

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "tesseract",
        "linux,mac,windows",
        "tesseract",
        "--enable-libtesseract",
        body,
    );
    x.src = include_str!("tesseract.rs");
    x
}

/// 构建 libtesseract / Build libtesseract
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    let cm = src.join("CMakeLists.txt");
    // check_leptonica_tiff_support() 的 try_run 连生成都过不了（leptonica
    // 导出目标引用 WebP::webp 却没有 find_package）——去掉探测
    // its try_run fails to even generate (leptonica's exported targets
    // reference WebP::webp without find_package) — drop the probe
    fsx::sed_i(
        "s/check_leptonica_tiff_support()/#check_leptonica_tiff_support()/",
        &cm,
        &src,
    )?;
    // 不构建 bin/tesseract：FFmpeg 只消费 libtesseract.a，而可执行文件的
    // 静态链接顺序无解（libwebp.a 的尾部出现会在组外拉入 SharpYuv* 引用）
    // don't build bin/tesseract: FFmpeg only needs libtesseract.a and the
    // binary's static link order is unfixable (libwebp.a's tail occurrence
    // pulls SharpYuv* refs outside the group)
    for pat in [
        "s/^add_executable(tesseract src\\/tesseract.cpp)$/#&/",
        "s/^target_link_libraries(tesseract libtesseract)$/#&/",
        "s/^  target_link_libraries(tesseract \\${TIFF_LIBRARIES})$/#&/",
        "s/^  target_link_libraries(tesseract pthread)$/#&/",
        "s/^install(TARGETS tesseract DESTINATION bin)$/#&/",
    ] {
        fsx::sed_i(pat, &cm, &src)?;
    }
    // M_PI 在严格 -std=c++20 + mingw 头文件下要 _USE_MATH_DEFINES 才
    // 现身（baseapi.cpp:2284 裸用它）；其它平台定义了也不碍事 /
    // M_PI only appears with _USE_MATH_DEFINES under strict -std=c++20
    // + mingw headers (baseapi.cpp:2284 uses it bare); harmless where
    // it's already defined
    let _g = sys::EnvGuard::append(&[("CXXFLAGS", "-D_USE_MATH_DEFINES")]);
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_TRAINING_TOOLS=OFF",
            "-DSW_BUILD=OFF",
            "-DBUILD_TESTS=OFF",
            "-DDISABLE_ARCHIVE=ON",
            "-DDISABLE_CURL=ON",
        ],
    )?;
    // tesseract.pc 不总带 C++ 运行时；并入已有 Libs.private 键（上游发
    // 空键，裸追加会产生重复字段）/ tesseract.pc does not always carry the
    // C++ runtime; merge into the existing Libs.private key (a raw append
    // would duplicate the field)
    pkgcfg::libs_private("tesseract", &[&e.cxxrt]);
    Ok(())
}
