//! vulkan 配方：Vulkan 头文件 + BtbN 的 Vulkan-Shim-Loader（shim 冒充
//! 真加载器 libvulkan.a + vulkan.pc；本配方把它的急切构造函数改成首次
//! 调用才 dlopen/LoadLibrary——见下方 gen_shim.py 补丁注释）
//! vulkan recipe: Vulkan headers + BtbN's Vulkan-Shim-Loader (the shim
//! impersonates the real loader — libvulkan.a + vulkan.pc; this recipe
//! converts its eager constructor into first-call lazy
//! dlopen/LoadLibrary — see the gen_shim.py patch notes below)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("vulkan", "linux,windows", "vulkan", "--enable-vulkan", body);
    x.src = include_str!("vulkan.rs");
    x
}

/// 构建 Vulkan 头文件 + shim 加载器
/// Build the Vulkan headers + shim loader
fn body(ctx: &Ctx) -> Result<(), String> {
    // 第一段：Vulkan-Headers / part one: Vulkan-Headers
    let hdrs = ctx.src_named("vulkan-headers")?;
    buildsys::cmake_build(
        &hdrs,
        &[
            "-DVULKAN_HEADERS_ENABLE_MODULE=NO",
            "-DVULKAN_HEADERS_ENABLE_TESTS=NO",
            "-DVULKAN_HEADERS_ENABLE_INSTALL=YES",
        ],
    )?;
    // fetch 到的 src 目录挪开，让位给 shim 检出 / move the fetched src
    // aside for the shim checkout
    let headers = ctx.wd.join("headers");
    std::fs::rename(&hdrs, &headers).map_err(|e| format!("mv src headers: {e}"))?;
    // 第二段：Shim-Loader——它的 CMakeLists 会 add_subdirectory 一个
    // Vulkan-Headers 检出；git 把子模块路径物化成空目录，先删掉再把我们
    // 的头文件挪进去 / the shim's CMakeLists add_subdirectory()s a
    // Vulkan-Headers checkout; git materializes the submodule path as an
    // empty dir — remove it, then drop our headers in
    let shim = ctx.src_named("vulkan-shim")?;
    fsx::rm_rf(&shim.join("Vulkan-Headers"));
    std::fs::rename(&headers, shim.join("Vulkan-Headers"))
        .map_err(|e| format!("mv headers into shim: {e}"))?;
    // shim 的 gen_shim.py 生成 `__attribute__((constructor)) init()`，
    // 二进制 .init 阶段就急切 dlopen("libvulkan.so.1")——静态 glibc
    // 进程 dlopen 系统 .so 会拖入第二份 libc.so.6，其
    // __pthread_early_init 拿不到 auxv（_dl_pagesize 等未初始化）→
    // 除零 SIGFPE（x86_64-legacy gdb 实证：init→dlopen→
    // libvulkan.so.1→__pthread_early_init，连 ffplay -version 都
    // 必炸）。生成的蹦床本就 `if (likely(ptr_x))` 判空兜底，把
    // init 改成首次调用触发：去 constructor 属性、加只跑一次守卫、
    // 每个蹦床入口先 init()。竞态无害——重复 dlopen 同库返回同
    // 一 handle，指针重写为相同值。注意：这只救"启动即炸"，静态
    // glibc 本来就不支持 dlopen——真调 vulkan（ffmpeg -init_hw_
    // device vulkan / whisper GPU）在静态 linux 上仍会因双 libc
    // 崩，属静态构建固有边界 / the shim's gen_shim.py emits
    // `__attribute__((constructor)) init()` that eagerly dlopens
    // libvulkan.so.1 during .init — a static-glibc process dlopening
    // a system .so drags in a second libc.so.6 whose
    // __pthread_early_init gets no auxv (_dl_pagesize etc.
    // uninitialized) → div-by-zero SIGFPE (x86_64-legacy gdb
    // evidence: init→dlopen→libvulkan.so.1→__pthread_early_init —
    // even `ffplay -version` dies). The generated trampolines
    // already guard `if (likely(ptr_x))`, so flip init to
    // first-call: drop the constructor attribute, add a run-once
    // guard, call init() at every trampoline entry. The race is
    // benign — re-dlopen of the same lib returns the same handle
    // and pointers get rewritten with identical values. Note: this
    // only cures the boot-time crash; static glibc fundamentally
    // can't dlopen — actually USING vulkan (ffmpeg
    // -init_hw_device vulkan / whisper GPU) still crashes on
    // static linux, an inherent static-build boundary
    let genpy = shim.join("gen_shim.py");
    if genpy.is_file() {
        let s = fsx::read(&genpy);
        if s.contains("__attribute__((constructor))") {
            let ctor = "__attribute__((constructor))\nstatic void init() {";
            let tramp = "    print(f\"    if (likely(ptr_{name})) {{\")";
            if !s.contains(ctor) || !s.contains(tramp) {
                return Err(
                    "vulkan-shim gen_shim.py 布局已变：急切构造函数仍在但补丁锚点缺失，\
                     拒绝放行启动即崩的 dlopen / layout changed: eager constructor still \
                     present but patch anchor missing — refusing to ship a binary that \
                     crashes at boot"
                        .into(),
                );
            }
            let s = s.replace(
                ctor,
                concat!(
                    "static int vkshim_init_done = 0;\n",
                    "static void init() {\n",
                    "    if (vkshim_init_done)\n",
                    "        return;\n",
                    "    vkshim_init_done = 1;",
                ),
            );
            let s = s.replace(
                tramp,
                concat!(
                    "    print(f\"    if (unlikely(!vkshim_init_done))\")\n",
                    "    print(f\"        init();\")\n",
                    "    print(f\"    if (likely(ptr_{name})) {{\")",
                ),
            );
            fsx::write(&genpy, &s)?;
        }
    }
    buildsys::cmake_build(&shim, &["-DVULKAN_SHIM_IMPERSONATE=ON"])
}
