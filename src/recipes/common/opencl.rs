//! opencl 配方：OpenCL 头文件 + 静态 ICD 加载器（运行时才找厂商驱动）
//! opencl recipe: OpenCL headers + static ICD loader (vendor drivers
//! found at run time)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("opencl", "linux,windows", "OpenCL", "--enable-opencl", body);
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
    let mut args = vec![
        format!("-DOPENCL_ICD_LOADER_HEADERS_DIR={}/include", e.prefix),
        "-DOPENCL_ICD_LOADER_BUILD_SHARED_LIBS=OFF".to_string(),
        "-DOPENCL_ICD_LOADER_DISABLE_OPENCLON12=ON".to_string(),
        "-DOPENCL_ICD_LOADER_PIC=ON".to_string(),
        "-DOPENCL_ICD_LOADER_BUILD_TESTING=OFF".to_string(),
        "-DBUILD_TESTING=OFF".to_string(),
    ];
    if e.os == sys::Os::Windows {
        // 上游 master 的 cllayerinfo.exe（诊断工具，FFmpeg 不需要）用
        // StringFromGUID2 却没链 ole32——ld.bfd 时代靠默认库侥幸蒙混，
        // clangarm64 的 ld.lld 严格判 undefined 直接暴毙。FFmpeg 只要
        // 静态库本身，但 cmake 没有关掉该工具的开关，全局 exe 链接缀
        // 是最小侵入修复（对静态库目标零影响）/ upstream master's
        // cllayerinfo.exe (a diagnostic tool FFmpeg never needs) calls
        // StringFromGUID2 without linking ole32 — ld.bfd used to skate
        // by on default libs, clangarm64's ld.lld strictly convicts
        // undefined symbols. FFmpeg only wants the static lib, but cmake
        // offers no switch for the tool, so a global exe linker flag is
        // the least invasive fix (zero effect on the static-lib target)
        args.push("-DCMAKE_EXE_LINKER_FLAGS=-lole32".to_string());
    }
    let argrefs: Vec<&str> = args.iter().map(String::as_str).collect();
    buildsys::cmake_build(&icd, &argrefs)?;
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
    // 私有链接尾巴按平台给：windows 的 ICD 走注册表/COM；linux 上加载
    // 器 dlopen 厂商 ICD 只要 -ldl -lpthread / the private link tail is
    // platform-picked: the Windows ICD walks the registry/COM; on linux
    // the loader dlopen()s vendor ICDs and only needs -ldl -lpthread
    let tail = if e.os == sys::Os::Windows {
        "-lole32 -lshlwapi -lcfgmgr32"
    } else {
        "-ldl -lpthread"
    };
    fsx::write(
        &libdir.join("pkgconfig/OpenCL.pc"),
        &format!(
            "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: OpenCL\nDescription: OpenCL ICD Loader\nVersion: 9999\nCflags: -I${{includedir}}\nLibs: -L${{libdir}} {libarg}\nLibs.private: {tail}\n",
            e.prefix
        ),
    )
}
