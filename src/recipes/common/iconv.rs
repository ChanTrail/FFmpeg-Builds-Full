//! iconv 配方：GNU libiconv（base.yaml 恒启的字符集底座）
//! iconv recipe: GNU libiconv (the always-on charset base from base.yaml)
//!
//! 为什么必须自建：没有自编 libiconv 时，FFmpeg 的 `--enable-iconv` 会退到
//! glibc 内置 iconv——它运行时 dlopen gconv 模块（ISO_6937.so 等），模块
//! 再把宿主 glibc 拖进已内嵌另一版 glibc 的静态 ffmpeg 进程；双 glibc
//! 共存在 mpegts 解 SDT/EIT 的 DVB 文本时必崩 SIGFPE(FPE_INTDIV)。自编
//! libiconv 是独立实现，不走 gconv、不 dlopen——静态二进制才真正无外部
//! 运行时依赖。
//! 位置约束：必须早于 libass/libxml2 等潜在 iconv 消费者构建（ORDER 首
//! 位），否则它们的 .a 会绑到 glibc iconv，把同样的雷带进最终二进制。
//! Windows 走 MSYS2 预编译包、macOS 用 SDK 的 libSystem iconv，均无需
//! 本配方。
//!
//! Why it must be self-built: without our own libiconv, FFmpeg's
//! `--enable-iconv` falls back to glibc's built-in iconv — which dlopens
//! gconv modules (ISO_6937.so etc.) at runtime; such a module then drags
//! the host glibc into a static ffmpeg that already embeds a different
//! glibc, and the dual-glibc coexistence deterministically crashes with
//! SIGFPE (FPE_INTDIV) when mpegts parses SDT/EIT DVB text. Self-built
//! libiconv is self-contained — no gconv, no dlopen — so the static binary
//! is genuinely free of external runtime deps.
//! Ordering: must build BEFORE potential iconv consumers like libass/
//! libxml2 (first in ORDER), otherwise their .a archives bind to glibc
//! iconv and carry the same landmine into the final binary. Windows gets
//! libiconv from the MSYS2 package and macOS uses the SDK's libSystem
//! iconv — neither needs this recipe.

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("iconv", "linux", "file:lib/libiconv.a", "-", body);
    x.src = include_str!("iconv.rs");
    x
}

/// 构建 GNU libiconv / Build GNU libiconv
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // --disable-nls：不引 gettext 依赖、不装 locale 目录（与其它配方一致）
    // --disable-nls: no gettext dependency, no locale files (same as other recipes)
    buildsys::autotools_build(&src, &["--disable-nls"])
}
