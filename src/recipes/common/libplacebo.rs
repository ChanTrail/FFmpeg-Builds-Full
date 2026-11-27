//! libplacebo 配方（linux/mac）：Vulkan 滤镜栈——vf_libplacebo 色调
//! 映射 + ffplay -enable_vulkan HDR 渲染器的根基。与 windows 版的路
//! 径分歧：那边用 pacman shaderc + SPIRV-Cross/d3d11；这边 glslang
//! 编译器（自建仓）+ 无 d3d11/opengl，DV 链（libdovi）全接。
//! libplacebo recipe (linux/mac): the Vulkan filter stack — the
//! foundation of vf_libplacebo tone mapping and ffplay's
//! -enable_vulkan HDR renderer. Path differences from the windows
//! build: that one uses pacman's shaderc + SPIRV-Cross/d3d11; here the
//! compiler is our self-built glslang with no d3d11/opengl, and the DV
//! chain (libdovi) is fully wired.

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libplacebo",
        "linux,mac",
        "libplacebo",
        "--enable-libplacebo",
        body,
    );
    x.src = include_str!("libplacebo.rs");
    x
}

/// 构建 libplacebo / Build libplacebo
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    sys::run_ok(
        "git",
        &[
            "submodule",
            "update",
            "--init",
            "--recursive",
            "-q",
            "--depth=1",
            "--filter=blob:none",
        ],
        &src,
    )?;
    // utils_gen.py 把 ElementTree.parse 返回值（ElementTree）喂给要
    // Element 的 VkXML——getroot() 才是本意 / utils_gen.py feeds
    // ElementTree.parse's return (an ElementTree) to VkXML, which wants
    // an Element — getroot() is what it meant
    fsx::sed_i(
        "s/VkXML(ET\\.parse(xmlfile))/VkXML(ET.parse(xmlfile).getroot())/",
        &src.join("src/vulkan/utils_gen.py"),
        &src,
    )?;
    // glslang 的 meson 发现是 find_library 裸搜：SPIRV 等带 dirs 的走
    // -Dvulkan-sdk=<前缀>/lib 显式目录，而不带 dirs 的（glslang、
    // glslang-default-resource-limits）退化到 `-l` 试链——LIBRARY_PATH
    // 是 gcc 原生链接搜索路径，试链即中；build_info.h 头检查同理吃
    // CPATH。两变量都是编译器原生语义，meson 怎么内部实现都穿得过去 /
    // glslang's meson discovery is a bare find_library search: the
    // dirs-carrying lookups (SPIRV & co) hit the explicit
    // -Dvulkan-sdk=<prefix>/lib directory, while the dir-less ones
    // (glslang, glslang-default-resource-limits) fall back to a `-l`
    // test link — LIBRARY_PATH is gcc's native link search path, so the
    // test link lands; the build_info.h header check likewise rides
    // CPATH. Both variables are compiler-native semantics, so they hold
    // regardless of meson's internal implementation
    let libpath = match std::env::var("LIBRARY_PATH") {
        Ok(old) if !old.is_empty() => format!("{}/lib:{old}", e.prefix),
        _ => format!("{}/lib", e.prefix),
    };
    let cpath = match std::env::var("CPATH") {
        Ok(old) if !old.is_empty() => format!("{}/include:{old}", e.prefix),
        _ => format!("{}/include", e.prefix),
    };
    let _g = sys::EnvGuard::set(&[("LIBRARY_PATH", libpath), ("CPATH", cpath)]);
    buildsys::meson_build(
        &src,
        &[
            "-Dvulkan=enabled",
            "-Dvk-proc-addr=enabled",
            &format!(
                "-Dvulkan-registry={}/share/vulkan/registry/vk.xml",
                e.prefix
            ),
            &format!("-Dvulkan-sdk={}", e.prefix),
            // glslang 自建仓唯一着色器编译器；shaderc 在 linux/mac 无
            // 现成链（要拖 SPIRV-Tools+abseil）/ our self-built glslang
            // is the only shader compiler; shaderc has no lightweight
            // chain on linux/mac (it would drag SPIRV-Tools+abseil)
            "-Dshaderc=disabled",
            "-Dglslang=enabled",
            // d3d11 是 windows 专属；opengl 后端 ffmpeg 用不上
            // （vf_libplacebo 只走 vulkan），砍掉缩小面 / d3d11 is
            // windows-only; ffmpeg has no use for the opengl backend
            // (vf_libplacebo is vulkan-only) — cut both to shrink the
            // surface
            "-Dd3d11=disabled",
            "-Dopengl=disabled",
            "-Dlcms=enabled",
            "-Dxxhash=enabled",
            "-Ddovi=enabled",
            "-Dlibdovi=enabled",
            "-Dunwind=disabled",
            "-Ddemos=false",
            "-Dtests=false",
            "-Dbench=false",
            "-Dfuzz=false",
        ],
    )?;
    // glslang 链是 C++——c++ 运行时进 Libs.private 兜底 /
    // the glslang chain is C++ — the runtime goes into Libs.private as
    // the backstop
    pkgcfg::libs_private("libplacebo", &[&e.cxxrt]);
    Ok(())
}
