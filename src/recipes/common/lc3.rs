//! lc3 配方：LC3/LC3plus 蓝牙音频
//! lc3 recipe: LC3/LC3plus Bluetooth audio

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "lc3",
        "linux,mac",
        "lc3,hdrsym:lc3.h@lc3_hr_setup_encoder,arsym:lib/liblc3.a@lc3_hr_setup_encoder",
        "--enable-liblc3",
        body,
    );
    x.src = include_str!("lc3.rs");
    x
}

/// 构建 liblc3 / Build liblc3
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    buildsys::meson_build(&src, &["-Dtools=false", "-Dpython=false"])
}
