//! opencl 配方：OpenCL 头文件 + 静态 ICD 加载器（运行时才找厂商驱动；
//! 旧 80-windows.sh 移植）
//! opencl recipe: OpenCL headers + static ICD loader (vendor drivers
//! found at run time; port of 80-windows.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("opencl", "windows", "OpenCL", "--enable-opencl", body);
    x.src = include_str!("opencl.rs");
    x
}

/// 构建 OpenCL 头 + ICD 加载器 / Build the OpenCL headers + ICD loader
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let hdrs = ctx.src_named("opencl-headers")?;
    let cl = e.prefix_win.join("include/CL");
    fsx::install_d(&cl)?;
    for f in fsx::walk(&hdrs.join("CL")) {
        if let Ok(rel) = f.strip_prefix(hdrs.join("CL")) {
            fsx::install(&f, &cl.join(rel))?;
        }
    }
    fsx::rm_rf(&hdrs);
    let icd = ctx.src_named("opencl-icd")?;
    buildsys::cmake_build(
        &icd,
        &[
            &format!("-DOPENCL_ICD_LOADER_HEADERS_DIR={}/include", e.prefix),
            "-DOPENCL_ICD_LOADER_BUILD_SHARED_LIBS=OFF",
            "-DOPENCL_ICD_LOADER_DISABLE_OPENCLON12=ON",
            "-DOPENCL_ICD_LOADER_PIC=ON",
            "-DOPENCL_ICD_LOADER_BUILD_TESTING=OFF",
            "-DBUILD_TESTING=OFF",
        ],
    )?;
    let libdir = e.prefix_win.join("lib");
    let libarg = if libdir.join("libOpenCL.a").is_file() {
        "-lOpenCL"
    } else if libdir.join("OpenCL.a").is_file() {
        "-l:OpenCL.a"
    } else {
        return Err(format!(
            "OpenCL ICD loader archive not found in {}",
            libdir.display()
        ));
    };
    fsx::write(
        &libdir.join("pkgconfig/OpenCL.pc"),
        &format!(
            "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: OpenCL\nDescription: OpenCL ICD Loader\nVersion: 9999\nCflags: -I${{includedir}}\nLibs: -L${{libdir}} {libarg}\nLibs.private: -lole32 -lshlwapi -lcfgmgr32\n",
            e.prefix
        ),
    )
}
