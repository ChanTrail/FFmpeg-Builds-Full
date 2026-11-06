//! modplug 配方：MOD 音乐播放（旧 30-audio.sh 移植）
//! modplug recipe: MOD music playback (port of 30-audio.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "modplug",
        "linux,mac",
        "libmodplug",
        "--enable-libmodplug",
        body,
    );
    x.src = include_str!("modplug.rs");
    x
}

/// 构建 libmodplug / Build libmodplug
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 用了 `register` 的 C++98 代码，现代默认标准会拒绝
    // C++98 code using `register`, which modern default standards reject
    let _g = sys::EnvGuard::append(&[("CXXFLAGS", "-std=gnu++11 -Wno-register")]);
    buildsys::autotools_build(&src, &[])
}
