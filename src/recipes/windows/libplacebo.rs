//! libplacebo 配方：SPIRV-Cross + libplacebo（Vulkan 滤镜栈；MSYS2 的
//! shaderc.pc 指向共享库而 FFmpeg 静态链接，改用 combined 静态版；旧
//! 80-windows.sh 移植）
//! libplacebo recipe: SPIRV-Cross + libplacebo (Vulkan filter stack;
//! MSYS2's shaderc.pc points at the shared lib but FFmpeg links
//! statically, so substitute the combined static one; port of
//! 80-windows.sh)

use crate::buildsys;
use crate::extras::{self, Ctx, Recipe};
use crate::fsx;
use crate::pkgcfg;
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "libplacebo",
        "windows",
        "libplacebo",
        "--enable-libplacebo",
        body,
    );
    x.src = include_str!("libplacebo.rs");
    x
}

/// 构建 SPIRV-Cross + libplacebo / Build SPIRV-Cross + libplacebo
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // vendored 清单时间戳：产物全落 pacman 拥有路径，overlay 排除规则会
    // 挡在车外——先记时间戳，末尾落清单豁免 / stamp now; the manifest at
    // the end exempts these pacman-owned paths from the overlay exclusion
    let stamp = ctx.stamp();
    let stamp_t = std::fs::metadata(&stamp)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::now());
    // -Dd3d11=enabled 需要 spirv-cross-c-shared pc 模块；MSYS2 只发 DLL
    // 导入库——静态构建 SPIRV-Cross 并手写 .pc / -Dd3d11=enabled needs a
    // spirv-cross-c-shared module; MSYS2 ships only the DLL import lib —
    // build SPIRV-Cross static and write the .pc by hand
    let sc = ctx.src_named("spirv-cross")?;
    let cm = fsx::read(&sc.join("CMakeLists.txt"));
    let grab = |key: &str| -> String {
        cm.lines()
            .find_map(|l| {
                if l.contains(&format!("set(spirv-cross-abi-{key}")) {
                    l.split(|c: char| !c.is_ascii_digit())
                        .find(|t| !t.is_empty())
                        .map(String::from)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "0".into())
    };
    let (vmaj, vmin, vpch) = (grab("major"), grab("minor"), grab("patch"));
    buildsys::cmake_build(
        &sc,
        &[
            "-DSPIRV_CROSS_SHARED=OFF",
            "-DSPIRV_CROSS_STATIC=ON",
            "-DSPIRV_CROSS_CLI=OFF",
            "-DSPIRV_CROSS_ENABLE_TESTS=OFF",
            "-DSPIRV_CROSS_FORCE_PIC=ON",
            "-DSPIRV_CROSS_ENABLE_CPP=OFF",
        ],
    )?;
    fsx::write(
        &e.prefix_win.join("lib/pkgconfig/spirv-cross-c-shared.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include/spirv_cross\n\nName: spirv-cross-c-shared\nDescription: C API for SPIRV-Cross (static)\nVersion: {vmaj}.{vmin}.{vpch}\nLibs: -L${{libdir}} -lspirv-cross-c -lspirv-cross-glsl -lspirv-cross-hlsl -lspirv-cross-reflect -lspirv-cross-msl -lspirv-cross-util -lspirv-cross-core {}\nCflags: -I${{includedir}}\n",
            e.prefix, e.cxxrt
        ),
    )?;
    fsx::rm_rf(&sc);
    // 钉 release tag 而非 master：master 每次上游提交都让期望 SHA 失配；
    // tag 不可变 / pin a release tag, not master: every upstream commit
    // breaks the expected-SHA check; tags are immutable
    let src = ctx.src()?;
    sys::run_ok(
        "git",
        &[
            "submodule",
            "update",
            "--init",
            "--recursive",
            "--depth=1",
            "--filter=blob:none",
        ],
        &src,
    )?;
    // MSYS2 的 shaderc.pc 指向共享库——用 combined 静态版顶替
    // MSYS2's shaderc.pc points at the shared lib — substitute the combined
    // static one
    let pcd = e.prefix_win.join("lib/pkgconfig");
    let comb = pcd.join("shaderc_combined.pc");
    if comb.is_file() {
        std::fs::copy(&comb, pcd.join("shaderc.pc")).map_err(|e| format!("cp shaderc.pc: {e}"))?;
    }
    pkgcfg::libs_private("shaderc", &[&e.cxxrt]);
    // utils_gen.py 把 ElementTree.parse 返回值（ElementTree）喂给要
    // Element 的 VkXML——getroot() 才是本意 / utils_gen.py feeds
    // ElementTree.parse's return (an ElementTree) to VkXML, which wants an
    // Element — getroot() is what it meant
    fsx::sed_i(
        "s/VkXML(ET\\.parse(xmlfile))/VkXML(ET.parse(xmlfile).getroot())/",
        &src.join("src/vulkan/utils_gen.py"),
        &src,
    )?;
    buildsys::meson_build(
        &src,
        &[
            "-Dvulkan=enabled",
            "-Dvk-proc-addr=enabled",
            &format!(
                "-Dvulkan-registry={}/share/vulkan/registry/vk.xml",
                e.prefix
            ),
            "-Dshaderc=enabled",
            "-Dglslang=disabled",
            "-Dlcms=enabled",
            "-Dxxhash=enabled",
            "-Dd3d11=enabled",
            "-Ddemos=false",
            "-Dtests=false",
            "-Dbench=false",
            "-Dfuzz=false",
        ],
    )?;
    pkgcfg::libs_private("libplacebo", &[&e.cxxrt]);
    extras::vendored_manifest("lib/.vendored-libplacebo", stamp_t)
}
