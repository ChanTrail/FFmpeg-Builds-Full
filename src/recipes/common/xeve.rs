//! xeve 配方：EVC 编码器。注意：产物 libxeve.a 与 libxevd.a 共享
//! mc/recon 源——windows fixup 与 mac nmedit 会对交集符号做后缀改名/
//! 本地化，归档必须保持出厂状态（改过的快照曾把 alf_create 改成现存
//! API 名 xeve_alf_create 而自毁）/ xeve recipe: EVC encoder. Note:
//! libxeve.a shares mc/recon sources with libxevd.a — windows fixup and
//! mac nmedit suffix-rename/localize the intersecting symbols, so the
//! archived product must stay pristine (a cached snapshot once renamed
//! alf_create into the live API name xeve_alf_create and self-destructed)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys::{self, Arch, Os};

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("xeve", "all", "xeve", "--enable-libxeve", body);
    x.src = include_str!("xeve.rs");
    x
}

/// 构建 xeve / Build xeve
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    fsx::write(&src.join("version.txt"), "v0.7.1\n")?;
    let cm = src.join("CMakeLists.txt");
    // 与 xevd 同款无条件 -static bug / same unconditional -static linker
    // bug as xevd
    fsx::sed_i("s/set(CMAKE_EXE_LINKER_FLAGS \"-static\")//", &cm, &src)?;
    let mut arm: Vec<String> = Vec::new();
    let neon = src.join("src_base/neon/sse2neon.h");
    if e.arch == Arch::Arm64 {
        // 0.7 之前的捆绑 sse2neon 给 __builtin_nontemporal_store 传了不匹配
        // 的向量类型；强制走 #else 普通 vst1q 存储 / pre-0.7 bundled
        // sse2neon passes mismatched vector types to
        // __builtin_nontemporal_store; force the #else plain vst1q branch
        if neon.is_file() {
            let _ = fsx::sed_i(
                "s/__has_builtin(__builtin_nontemporal_store)/0/",
                &neon,
                &src,
            );
        }
        arm.push("-DARM=TRUE".to_string());
    }
    if e.os == Os::Windows && neon.is_file() {
        // 捆绑 sse2neon 用 posix_memalign/free 实现 _mm_malloc/_mm_free——
        // mingw CRT 都没有。转到 _aligned_* 对（并去掉 align==1 快路径，
        // 否则 _aligned_free 会错配普通 malloc）/ bundled sse2neon uses
        // posix_memalign/free for _mm_malloc/_mm_free — neither exists in
        // the mingw CRT. Route through the _aligned_* pair (and drop the
        // align==1 fast path which would mis-pair _aligned_free)
        fsx::sed_i("/#include <stdlib.h>/a\\\n#include <malloc.h>", &neon, &src)?;
        fsx::sed_i(
            "s|if (align == 1)|if (0) /* was: align == 1 malloc fast-path — pairs must be _aligned_ */|;\
             s|if (!posix_memalign(&ptr, align, size))|if ((ptr = _aligned_malloc(size, align)) != NULL)|;\
             s|free(addr);|_aligned_free(addr);|",
            &neon,
            &src,
        )?;
    }
    let static_app = if e.os == Os::Mac { "OFF" } else { "ON" };
    // 与 xevd 同款的 app 目录问题 / same app-dir problem as xevd
    fsx::sed_i(
        "s/^add_subdirectory(app)/#add_subdirectory(app)/",
        &cm,
        &src,
    )?;
    let cflags = std::env::var("CFLAGS").unwrap_or_default();
    let mut args = vec![
        "-DSET_PROF=MAIN".to_string(),
        format!("-DXEVE_APP_STATIC_BUILD={static_app}"),
        format!("-DCMAKE_C_FLAGS={cflags} -w"),
    ];
    args.extend(arm);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    buildsys::cmake_build(&src, &refs)?;
    fsx::rm_glob(&e.prefix_win.join("lib"), "libxeve.so*");
    fsx::rm_glob(&e.prefix_win.join("lib"), "libxeve*.dylib");
    fsx::rm_glob(&e.prefix_win.join("lib"), "libxeve.dll*");
    fsx::rm_glob(&e.prefix_win.join("bin"), "libxeve*.dll*");
    let sub = e.prefix_win.join("lib/xeve/libxeve.a");
    let flat = e.prefix_win.join("lib/libxeve.a");
    if sub.is_file() && !flat.is_file() {
        std::fs::copy(&sub, &flat).map_err(|x| format!("copy xeve.a: {x}"))?;
    }
    Ok(())
}
