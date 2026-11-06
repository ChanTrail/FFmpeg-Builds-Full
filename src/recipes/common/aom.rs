//! aom 配方：AV1 编解码（Windows 用源码自建代替 MSYS2 包：UCRT64 预编
//! 译版在链进大号静态 ffmpeg 时在 aom_codec_enc_init 内崩溃——嫌疑在
//! MSYS2 构建旗标而非上游源码；旧 50-video.sh 移植）
//! aom recipe: AV1 codec (vendored on Windows because the UCRT64 prebuilt
//! crashes inside aom_codec_enc_init in our fat static ffmpeg — the
//! suspect is MSYS2's build flags, not upstream; port of 50-video.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("aom", "windows:x86_64", "aom", "--enable-libaom", body);
    x.src = include_str!("aom.rs");
    x
}

/// 构建 libaom / Build libaom
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 不要 examples/tools/tests：只要库+头。nasm 在 x86_64 自动探测；
    // aarch64 用 aom 自己的汇编（不要强加 -DENABLE_NASM=ON）/ library +
    // headers only. nasm auto-detected on x86_64; aarch64 uses aom's own
    // asm (do NOT force ENABLE_NASM=ON — it makes nasm mandatory)
    buildsys::cmake_build(
        &src,
        &[
            "-DENABLE_EXAMPLES=OFF",
            "-DENABLE_TESTS=OFF",
            "-DENABLE_TOOLS=OFF",
            "-DENABLE_DOCS=OFF",
        ],
    )
}
