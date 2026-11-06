//! lensfun 配方：镜头畸变校正（master 快照；FFmpeg 探测 lf_db_create；
//! 旧 45-glib-stack.sh 移植）
//! lensfun recipe: lens distortion correction (master snapshot; FFmpeg
//! probes lf_db_create; port of 45-glib-stack.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "lensfun",
        "all",
        "lensfun,hdrsym:lensfun/lensfun.h@lf_db_create",
        "--enable-liblensfun",
        body,
    );
    x.src = include_str!("lensfun.rs");
    x
}

/// 构建 lensfun master 快照 / Build a lensfun master snapshot
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // FFmpeg 探测 lf_db_create——只在 0.3.99 开发线存在，锁定 master
    // FFmpeg probes lf_db_create — only on the 0.3.99 dev line; pin master
    let src = ctx.src()?;
    // INSTALL_PYTHON_MODULE 并不真关 python 构建——只要找到 python3 就跑
    // setup.py。PYTHON=FALSE 跳过 find 和 IF(PYTHON) 块。
    // *_STATIC_COMPILATION：lensfun.cpp 包含 glib 头，Windows 上缺这些宏
    // 会留 __imp_g_* 引用。
    // INSTALL_PYTHON_MODULE doesn't really gate the python build — apps/
    // CMakeLists runs setup.py whenever python3 is found; PYTHON=FALSE
    // skips both. *_STATIC_COMPILATION: lensfun.cpp includes glib headers,
    // which otherwise leave __imp_g_* refs under a -static link.
    let _g = sys::EnvGuard::append(&[
        (
            "CFLAGS",
            "-DGLIB_STATIC_COMPILATION -DGOBJECT_STATIC_COMPILATION -DGIO_STATIC_COMPILATION -DGMODULE_STATIC_COMPILATION",
        ),
        (
            "CXXFLAGS",
            "-DGLIB_STATIC_COMPILATION -DGOBJECT_STATIC_COMPILATION -DGIO_STATIC_COMPILATION -DGMODULE_STATIC_COMPILATION",
        ),
    ]);
    buildsys::cmake_build(
        &src,
        &[
            "-DBUILD_STATIC=ON",
            "-DBUILD_TESTS=OFF",
            "-DBUILD_LENSTOOL=OFF",
            "-DBUILD_DOC=OFF",
            "-DINSTALL_PYTHON_MODULE=OFF",
            "-DINSTALL_HELPER_SCRIPTS=OFF",
            "-DPYTHON=FALSE",
        ],
    )?;
    // 上游不总装 .pc；FFmpeg 靠 pkg-config 探测
    // upstream does not always install a .pc; FFmpeg probes by pkg-config
    let pcd = e.prefix_win.join("lib/pkgconfig");
    if !pcd.join("lensfun.pc").is_file() {
        fsx::write(
            &pcd.join("lensfun.pc"),
            &format!(
                "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: lensfun\nDescription: lens distortion database and correction library\nVersion: 0.3.99\nRequires.private: glib-2.0\nLibs: -L${{libdir}} -llensfun\nLibs.private: {} -lm -lpthread\nCflags: -I${{includedir}} -I${{includedir}}/lensfun\n",
                e.prefix, e.cxxrt
            ),
        )?;
    }
    Ok(())
}
