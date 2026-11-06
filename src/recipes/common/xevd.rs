//! xevd 配方：EVC 解码器（旧 50-video.sh 移植）
//! xevd recipe: EVC decoder (port of 50-video.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys::{self, Arch, Os};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "xevd",
        "linux,mac,windows",
        "xevd",
        "--enable-libxevd",
        body,
    );
    x.src = include_str!("xevd.rs");
    x
}

/// 构建 xevd / Build xevd
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    fsx::write(&src.join("version.txt"), "v0.8.0\n")?;
    let cm = src.join("CMakeLists.txt");
    // 无 -DARM 时其 cmake 选 x86 SIMD 路径在 aarch64 上编不了；但 ARM 分支
    // 又工程级地 `set(CMAKE_EXE_LINKER_FLAGS "-static")`——macOS 上没静态
    // libSystem，连编译器 ABI 检查都杀掉 / without -DARM their cmake picks
    // the x86 SIMD path; but the ARM branch also project-wide sets
    // `-static`, which has no libSystem to link on macOS and kills the
    // compiler ABI check itself
    fsx::sed_i("s/set(CMAKE_EXE_LINKER_FLAGS \"-static\")//", &cm, &src)?;
    let arm = if e.arch == Arch::Arm64 {
        "-DARM=TRUE"
    } else {
        ""
    };
    // *_APP_STATIC_BUILD 让演示程序 -static 链接——macOS 过不了
    // makes the demo app link -static; macOS can never pass that
    let static_app = if e.os == Os::Mac { "OFF" } else { "ON" };
    // app/xevd_app.c 需要 Darwin/ARM 上没有的平台时钟宏；FFmpeg 只要库
    // app/xevd_app.c needs platform clock macros absent on Darwin/ARM;
    // FFmpeg wants only the library
    fsx::sed_i(
        "s/^add_subdirectory(app)/#add_subdirectory(app)/",
        &cm,
        &src,
    )?;
    let cflags = std::env::var("CFLAGS").unwrap_or_default();
    let mut args = vec![
        "-DSET_PROF=MAIN".to_string(),
        format!("-DXEVD_APP_STATIC_BUILD={static_app}"),
        format!("-DCMAKE_C_FLAGS={cflags} -w"),
    ];
    if !arm.is_empty() {
        args.push(arm.to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    buildsys::cmake_build(&src, &refs)?;
    // 其 cmake 总会同时构建+装 `_dynamic` 共享目标——前缀里的 libxevd.so
    // 会把 -lxevd 变成运行期依赖 / its cmake always builds+installs a
    // `_dynamic` SHARED target too — libxevd.so turns -lxevd into a runtime
    // dependency
    fsx::rm_glob(&e.prefix_win.join("lib"), "libxevd.so*");
    fsx::rm_glob(&e.prefix_win.join("lib"), "libxevd*.dylib");
    fsx::rm_glob(&e.prefix_win.join("lib"), "libxevd.dll*");
    fsx::rm_glob(&e.prefix_win.join("bin"), "libxevd*.dll*");
    // 归档装在 lib/xevd/ 子目录而 xevd.pc 声明普通 ${libdir}——-lxevd
    // 解析不到 / the archive lands in lib/xevd/ while xevd.pc advertises
    // ${libdir} — -lxevd resolves to nothing
    let sub = e.prefix_win.join("lib/xevd/libxevd.a");
    let flat = e.prefix_win.join("lib/libxevd.a");
    if sub.is_file() && !flat.is_file() {
        std::fs::copy(&sub, &flat).map_err(|x| format!("copy xevd.a: {x}"))?;
    }
    Ok(())
}
