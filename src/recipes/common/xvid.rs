//! xvid 配方：Xvid MPEG-4 视频（autotools 根在 build/generic 子目录）
//! xvid recipe: Xvid MPEG-4 (its autotools root lives in build/generic)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("xvid", "linux,mac", "xvidcore", "--enable-libxvid", body);
    x.src = include_str!("xvid.rs");
    x
}

/// 构建 xvidcore / Build xvidcore
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = crate::sys::env();
    // 老代码 typedef bool 撞 C23 关键字——GCC16 默认 -std=gnu23，
    // 压回 gnu17 / its old `typedef ... bool` collides with the C23
    // keyword — GCC16 defaults to -std=gnu23, clamp it back to gnu17
    let _g = crate::sys::EnvGuard::append(&[("CFLAGS", "-std=gnu17")]);
    // 快照会恢复上次半成品 install 留下的 libxvidcore.so*——xvid 的
    // install 规则 `ln -s` 不带 -f，撞旧文件直接 Error 1。构建前清
    // 一遍；装完再清一遍（静态前缀本来就不该带 .so）/ snapshots can
    // restore libxvidcore.so* leftovers from a previous half-install —
    // xvid's install rule runs `ln -s` without -f and dies on them.
    // Clean once before building and again after install (a static
    // prefix should not carry .so files anyway)
    for p in crate::fsx::walk(&e.prefix_win.join("lib")) {
        if p.file_name()
            .map(|n| n.to_string_lossy().starts_with("libxvidcore.so"))
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&p);
        }
    }
    // autotools 工程嵌在 build/generic——不是仓库根。install 规则的
    // $(LN_S) 探测成不带 -f 的 `ln -s`，撞已存在的 .so.4 必炸——
    // 命令行变量 LN_S="ln -sf" 覆写掉（预清理是兜底，这里才是治本）/
    // the autotools project is nested under build/generic, not the
    // root. Its install rule's $(LN_S) probes to a flag-less `ln -s`
    // that dies on an existing .so.4 — override it with the
    // command-line var LN_S="ln -sf" (the pre-clean is belt; this is
    // the suspenders)
    buildsys::autotools_build_vars(&ctx.src()?.join("build/generic"), &[], &["LN_S=ln -sf"])?;
    for p in crate::fsx::walk(&e.prefix_win.join("lib")) {
        if p.file_name()
            .map(|n| n.to_string_lossy().starts_with("libxvidcore.so"))
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&p);
        }
    }
    // 上游不随包发 xvidcore.pc——pkg-config 探针全靠手写的这个 /
    // upstream ships no xvidcore.pc — the pkg-config probe relies on
    // this hand-written one
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/xvidcore.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: xvidcore\nDescription: Xvid MPEG-4 video codec\nVersion: 1.3.7\nLibs: -L${{libdir}} -lxvidcore\nLibs.private: -lm\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )
}
