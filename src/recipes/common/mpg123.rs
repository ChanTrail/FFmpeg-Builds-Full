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
    let e = crate::sys::env();
    let src = ctx.src()?;
    // --enable-components=libmpg123 实测会把 .pc 的 install 规则一并
    // 关掉——回归默认全编，audio 输出后端探测不到会自己降级 /
    // --enable-components=libmpg123 turns out to switch off the .pc
    // install rule along with the extras — back to a default full
    // build; missing audio backends degrade on their own
    buildsys::autotools_build(&src, &[])?;
    // 兜底：万一该版本仍不产 libmpg123.pc，手写补齐（lame 的
    // pkg-config 探测只靠它）/ fallback: if this build still shipped
    // no libmpg123.pc, fabricate one (lame's pkg-config probe lives
    // on it)
    let pc = e.prefix_win.join("lib/pkgconfig/libmpg123.pc");
    if !pc.is_file() {
        crate::fsx::write(
            &pc,
            &format!(
                "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: libmpg123\nDescription: MPEG-1/2/3 audio decoder library\nVersion: 1.33.7\nLibs: -L${{libdir}} -lmpg123\nLibs.private: -lm\nCflags: -I${{includedir}}\n",
                e.prefix
            ),
        )?;
    }
    Ok(())
}
