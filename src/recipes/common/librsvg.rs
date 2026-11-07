//! librsvg 配方：SVG 解码（Rust 部分经 cargo-c 驱动）
//! librsvg recipe: SVG decode, Rust part driven through cargo-c

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "librsvg",
        "linux,mac",
        "librsvg-2.0",
        "--enable-librsvg",
        body,
    );
    x.src = include_str!("librsvg.rs");
    x
}

/// 构建 librsvg / Build librsvg
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // 重建旧去重器处理过的归档：改赋值使 .ok 哈希失效
    // rebuild archives touched by old dedup passes: this assignment bumps
    // the .ok hash
    let _rebuild_rev = 4;
    // runner 没 cargo-cbuild 时把 cargo-c 装进前缀（PATH 已含且被 warm-cache
    // 携带）。版本钉死：librsvg 2.63.x 要 cargo-c >= 0.10.10
    // when the runner lacks cargo-cbuild, install cargo-c into the prefix
    // (already on PATH and carried by the warm cache). Pinned: librsvg
    // 2.63.x wants cargo-c >= 0.10.10
    if sys::which("cargo-cbuild").is_none() {
        sys::run_ok(
            "cargo",
            &[
                "install",
                "--locked",
                "cargo-c",
                "--version",
                "0.10.26",
                "--root",
                &e.prefix,
            ],
            &e.work,
        )?;
        // SAFETY：配方体在引擎单线程循环内执行 / SAFETY: recipe bodies run
        // inside the engine's single-threaded loop
        unsafe {
            std::env::set_var(
                "PATH",
                format!(
                    "{}:{}",
                    e.prefix_win.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
    }
    let src = ctx.src()?;
    buildsys::meson_build(
        &src,
        &[
            "-Ddocs=disabled",
            "-Dtests=false",
            "-Dintrospection=disabled",
            "-Dpixbuf=disabled",
            "-Dvala=disabled",
            "-Davif=disabled",
        ],
    )
}
