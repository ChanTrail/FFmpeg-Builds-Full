//! lilv 配方：LV2 插件宿主依赖链，全部静态（lv2/zix/serd/sord/sratom
//! 整链从 common.yaml 源自造）
//! lilv recipe: LV2 plugin host dependency chain, all static (the whole
//! lv2/zix/serd/sord/sratom chain builds from common.yaml sources)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("lilv", "all", "lilv-0", "--enable-lv2", body);
    x.src = include_str!("lilv.rs");
    x
}

/// 构建 lilv 依赖链 / Build the lilv dependency chain
fn body(ctx: &Ctx) -> Result<(), String> {
    // lv2 本身不带库——只有头文件 + lv2.pc。否则 sratom/lilv 会走 meson
    // wrap 解析它，永远不会装上 lilv-0.pc Requires 的那个 .pc / lv2 ships
    // no lib — headers + lv2.pc; otherwise sratom/lilv resolve it via a
    // meson wrap and the .pc lilv-0.pc Requires never lands
    let lv2 = ctx.src_named("lv2")?;
    buildsys::meson_build(
        &lv2,
        &["-Ddocs=disabled", "-Dtests=disabled", "-Dplugins=disabled"],
    )?;
    fsx::rm_rf(&lv2);
    for (name, opts) in [
        (
            "zix",
            "-Ddocs=disabled -Dbenchmarks=disabled -Dtests=disabled -Dtests_cpp=disabled",
        ),
        ("serd", "-Ddocs=disabled -Dtools=disabled -Dtests=disabled"),
        ("sord", "-Ddocs=disabled -Dtools=disabled -Dtests=disabled"),
        ("sratom", "-Ddocs=disabled -Dtests=disabled"),
        (
            "lilv",
            "-Ddocs=disabled -Dtools=disabled -Dtests=disabled -Dbindings_py=disabled",
        ),
    ] {
        crate::log::log(&format!("== {name}"));
        let src = ctx.src_named(name)?;
        let opts: Vec<&str> = opts.split_whitespace().collect();
        buildsys::meson_build(&src, &opts)?;
        fsx::rm_rf(&src);
    }
    Ok(())
}
