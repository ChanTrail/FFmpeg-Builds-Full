//! sndfile 配方：libsndfile（bs2b 的 configure 依赖）
//! sndfile recipe: libsndfile (a configure-time dependency of bs2b)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("sndfile", "linux,mac", "sndfile", "-", body);
    x.src = include_str!("sndfile.rs");
    x
}

/// 构建 libsndfile / Build libsndfile
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // ENABLE_EXTERNAL_LIBS=OFF：sndfile 从不 find_package(FLAC)，但开着时
    // 链接接口会把生成器表达式展开成 FLAC::FLAC——树里没有 libFLAC 时
    // generate 阶段必然 "target not found"
    // sndfile never calls find_package(FLAC) yet its link interface expands
    // to FLAC::FLAC when the option is on — guaranteed "target not found"
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_PROGRAMS=OFF",
            "-DBUILD_EXAMPLES=OFF",
            "-DBUILD_TESTING=OFF",
            "-DENABLE_BOW_DOCS=OFF",
            "-DINSTALL_PKGCONFIG_MODULE=ON",
            "-DENABLE_EXTERNAL_LIBS=OFF",
        ],
    )?;
    let pc = e.prefix_win.join("lib/pkgconfig/sndfile.pc");
    if !pc.is_file() {
        fsx::write(
            &pc,
            &format!(
                "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: sndfile\nDescription: Library for reading and writing audio files\nVersion: 1.2.2\nLibs: -L${{libdir}} -lsndfile\nLibs.private: -lm\nCflags: -I${{includedir}}\n",
                e.prefix
            ),
        )?;
    }
    Ok(())
}
