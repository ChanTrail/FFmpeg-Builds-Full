//! lensfun 配方：镜头畸变校正（master 快照；FFmpeg 探测 lf_db_create）
//! lensfun recipe: lens distortion correction (master snapshot; FFmpeg
//! probes lf_db_create)

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
    // 镜头库 XML 是运行时数据——lfDatabase::SystemLocation=
    // SYSTEM_DB_PATH（CMake_INSTALL_FULL_DATAROOTDIR/lensfun）默认
    // 烤 /opt/ffbuild/share/lensfun，用户机上查不到库，lensfun 滤镜
    // 全灭。指系统标准位：Linux 全系发行版 /usr/share/lensfun（
    // liblensfun-data 提供），macOS 随 Homebrew 约定 /opt/homebrew/
    // share/lensfun；SYSTEM_DB_UPDATE_PATH 上游已硬 /var，用户侧
    // ~/.local/share/lensfun 搜索链不受影响。DATAROOTDIR 兼安装目
    // 录，经 destdir stage 防写脏构建机。Windows 分支按上游相对
    // DATAROOTDIR 原样（模块相对寻址语义不同）/ the lens DB is
    // runtime data — lfDatabase::SystemLocation=SYSTEM_DB_PATH
    // (CMAKE_INSTALL_FULL_DATAROOTDIR/lensfun) defaults to baked
    // /opt/ffbuild/share/lensfun; user machines can't find it and the
    // lensfun filter is dead. Point at system-standard locations:
    // /usr/share/lensfun on every Linux distro (liblensfun-data),
    // /opt/homebrew/share/lensfun on macOS per Homebrew convention;
    // SYSTEM_DB_UPDATE_PATH is already /var upstream and the user-side
    // ~/.local/share/lensfun chain is untouched. DATAROOTDIR doubles
    // as the install dir — stage via destdir so the build machine
    // stays clean. Windows keeps upstream's relative DATAROOTDIR
    // (module-relative resolution semantics differ)
    let mut args = vec![
        "-DBUILD_STATIC=ON",
        "-DBUILD_TESTS=OFF",
        "-DBUILD_LENSTOOL=OFF",
        "-DBUILD_DOC=OFF",
        "-DINSTALL_PYTHON_MODULE=OFF",
        "-DINSTALL_HELPER_SCRIPTS=OFF",
        "-DPYTHON=FALSE",
    ];
    let dataroot: String;
    if e.os == crate::sys::Os::Mac {
        dataroot = "-DCMAKE_INSTALL_DATAROOTDIR=/opt/homebrew/share".into();
        args.push(dataroot.as_str());
    } else if e.os == crate::sys::Os::Linux {
        dataroot = "-DCMAKE_INSTALL_DATAROOTDIR=/usr/share".into();
        args.push(dataroot.as_str());
    }
    if e.os == crate::sys::Os::Windows {
        buildsys::cmake_build(&src, &args)?;
    } else {
        buildsys::cmake_build_destdir(&src, &args, &src.join("lfstage"))?;
    }
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
