//! libdovi 配方：Dolby Vision RPU 解析库（dovi_tool 仓 dolby_vision
//! 子crate 的 cargo-c capi——libplacebo 的 pl_hdr_metadata_from_dovi_
//! rpu 靠它提取 DV 亮度元数据；pkg-config 名 dovi、头落
//! include/libdovi/rpu_parser.h）
//! libdovi recipe: the Dolby Vision RPU parsing library (the cargo-c
//! capi of the dolby_vision subcrate in the dovi_tool repo — libplacebo's
//! pl_hdr_metadata_from_dovi_rpu uses it to extract DV brightness
//! metadata; pkg-config name is dovi, headers land in
//! include/libdovi/rpu_parser.h)

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    // windows 不走本配方——MSYS2 预编译 libdovi 包（GNU ABI rust 静态
    // 库 + dovi.pc）由 packages/windows.yaml 直供，MSYS2 自家
    // libplacebo 包同款依赖 / windows does not ride this recipe — MSYS2's
    // prebuilt libdovi package (a GNU-ABI rust static library plus
    // dovi.pc) is supplied directly through packages/windows.yaml, the
    // same dependency MSYS2's own libplacebo package declares
    let mut x = r(
        "dovi",
        "linux,mac",
        "dovi,hdrsym:libdovi/rpu_parser.h@dovi_parse_unspec62_nalu",
        "-",
        body,
    );
    x.src = include_str!("libdovi.rs");
    x
}

/// 构建 libdovi / Build libdovi
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // cargo-c 是 cargo 子命令，缺席就现装（rav1e 配方同款路径）/
    // cargo-c is a cargo subcommand — install it on demand (same path
    // as the rav1e recipe)
    if sys::which("cargo-cinstall").is_none() {
        sys::run_ok("cargo", &["install", "cargo-c", "--locked"], &ctx.wd)?;
    }
    // capi 实现在 dolby_vision/ 子crate——仓根是 dovi_tool CLI 二进制，
    // cinstall 必须进子目录。--features capi 显式开：cfg(cargo_c) 虽也
    // 能放进 capi 模块，但模块内部分函数依赖 feature 带入的 serde/linc
    // 通路，上游定义 capi = ["libc", "serde"] 才是本意 / the capi
    // implementation lives in the dolby_vision/ subcrate — the repo
    // root is the dovi_tool CLI binary, so cinstall must run inside.
    // --features capi is explicit: while cfg(cargo_c) also admits the
    // capi module, parts of it rely on the serde/libc paths the feature
    // pulls in, and upstream defines capi = ["libc", "serde"] as the
    // intended gate
    sys::run_ok(
        "cargo",
        &[
            "cinstall",
            "--release",
            "--features",
            "capi",
            "--prefix",
            &e.prefix,
            "--libdir",
            &format!("{}/lib", e.prefix),
            "--library-type=staticlib",
        ],
        &src.join("dolby_vision"),
    )
}
