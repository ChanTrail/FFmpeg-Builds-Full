//! mpg123 配方：MP1/2/3 解码库——lame 的 decoder 半边硬性要
//! libmpg123 >=1.26，缺了它的 configure 直接报错
//! mpg123 recipe: MPEG-1/2/3 decoder library — lame's decoder half
//! hard-requires libmpg123 >=1.26 and its configure errors out without it

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("mpg123", "linux,mac", "libmpg123", "-", body);
    x.src = include_str!("mpg123.rs");
    x
}

/// 构建 mpg123 / Build mpg123
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    // 只要 libmpg123——lame 探测的就是它；out123/命令行工具一律不编
    // （unknown option 的话 autotools 只警告继续，兜底全编也无碍）/
    // libmpg123 only — that's what lame probes for; skip the out123
    // modules and CLI (an unrecognised option only makes autotools warn
    // and carry on, worst case everything gets built)
    buildsys::autotools_build(&src, &["--enable-components=libmpg123"])
}
