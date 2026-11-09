//! vapoursynth 配方：帧服务端/插件宿主——核心绑定要 cython 生成，venv
//! 隔离绕开 PEP-668
//! vapoursynth recipe: the frameserver/plugin host — its core bindings
//! need cython; a venv sidesteps PEP-668

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "vapoursynth",
        // mac 出局：上游把整个库（含 .dylib）连同 pc 装进 python
        // site-packages——链进去的是 homebrew python 路径下的动态库，
        // 静态/运行时双无解 / mac is out: upstream installs the whole
        // lib (a .dylib) plus the pc into python's site-packages —
        // linking a dylib living under a homebrew python path is a
        // dead end both statically and at runtime
        "linux",
        "vapoursynth",
        "--enable-vapoursynth",
        body,
    );
    x.src = include_str!("vapoursynth.rs");
    x
}

/// 构建 VapourSynth / Build VapourSynth
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // cython 生成 src/cython 绑定——系统 python 受 PEP-668 保护不让
    // pip install，进 venv / cython generates the src/cython bindings —
    // system python is PEP-668-managed and refuses pip installs, so use
    // a venv
    let venv = ctx.wd.join("venv");
    sys::run_ok("python3", &["-m", "venv", &venv.to_string_lossy()], &ctx.wd)?;
    sys::run_ok(
        &format!("{}/bin/pip", venv.display()),
        &["install", "cython"],
        &ctx.wd,
    )?;
    // 子进程 PATH 是 sys::cmd 从 env.child_path 定死的——EnvGuard 改
    // 进程内 PATH 传不下去；child_path 含 <prefix>/bin，把 cython 脚本
    // 软链进去（shebang 指向 venv python，挪窝照跑）/ child PATH is
    // fixed by sys::cmd from env.child_path — an EnvGuard PATH tweak
    // never reaches the child; child_path does include <prefix>/bin so
    // symlink the cython script there (its shebang points at the venv
    // python and runs fine from anywhere)
    let bindir = e.prefix_win.join("bin");
    crate::fsx::install_d(&bindir)?;
    sys::run_ok(
        "ln",
        &[
            "-sf",
            &venv.join("bin/cython").to_string_lossy(),
            &bindir.join("cython").to_string_lossy(),
        ],
        &ctx.wd,
    )?;
    buildsys::meson_build(&src, &[])?;
    // 上游 meson 探到 python 后按"python 包"布局安装——静态库、头、
    // vapoursynth.pc 全落进 <prefix>/lib/python3/dist-packages/
    // vapoursynth/，pkg-config 根本扫不到。把三件东西搬回标准布局，
    // pc 按标准路径重写（原 pc 的 libdir 指着 dist-packages）/
    // once upstream meson finds python it installs in "python package"
    // layout — the static libs, headers and vapoursynth.pc all land
    // under <prefix>/lib/python3/dist-packages/vapoursynth/ where
    // pkg-config never looks. Relocate all three to the standard
    // layout; the pc is rewritten (the original's libdir points into
    // dist-packages)
    // dist-packages 还是 site-packages、python3 还是 3.x——随 runner 的
    // python 变。用 find 定位 vapoursynth.pc，取其安装根（pc 在
    // <root>/pkgconfig/ 下）/ dist-packages vs site-packages,
    // python3 vs 3.x — it varies with the runner's python. find the
    // installed vapoursynth.pc and take its install root (the pc sits
    // in <root>/pkgconfig/)
    let pc_hit = sys::capture(
        "find",
        &[
            &e.prefix_win.to_string_lossy(),
            "-name",
            "vapoursynth.pc",
            "-print",
            "-quit",
        ],
        &ctx.wd,
    )?;
    let pc_hit = pc_hit.trim();
    if pc_hit.is_empty() {
        return Err("vapoursynth: upstream installed no vapoursynth.pc".into());
    }
    let drop = std::path::Path::new(pc_hit)
        .parent()
        .and_then(|p| p.parent())
        .ok_or("vapoursynth: malformed vapoursynth.pc path")?
        .to_path_buf();
    sys::run_ok(
        "cp",
        &[
            "-R",
            &drop.join("include/.").to_string_lossy(),
            &e.prefix_win.join("include").to_string_lossy(),
        ],
        &ctx.wd,
    )?;
    // FFmpeg 探的是 <vapoursynth/VSScript4.h>——上游"python 包"布局可能
    // 干脆不装 C 头（实证：上面的 include/. 拷完该头仍缺）。源码树
    // include/ 自带全套 API 头，补拷成 include/vapoursynth/ 兜底 /
    // FFmpeg probes <vapoursynth/VSScript4.h> — upstream's
    // "python package" layout may skip the C headers entirely
    // (empirical: the header was still absent after the include/. copy
    // above). The source tree's include/ ships the full API header set;
    // land them under include/vapoursynth/ as a guaranteed fallback
    let vsh = e.prefix_win.join("include/vapoursynth");
    crate::fsx::install_d(&vsh)?;
    for h in crate::fsx::walk(&src.join("include")) {
        if h.extension().and_then(|x| x.to_str()) == Some("h")
            && let Some(name) = h.file_name()
        {
            crate::fsx::install(&h, &vsh.join(name))?;
        }
    }
    sys::run_ok(
        "cp",
        &[
            &drop.join("libvapoursynth.a").to_string_lossy(),
            &drop.join("libvsscript.a").to_string_lossy(),
            &e.prefix_win.join("lib").to_string_lossy(),
        ],
        &ctx.wd,
    )?;
    crate::fsx::write(
        &e.prefix_win.join("lib/pkgconfig/vapoursynth.pc"),
        &format!(
            "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: vapoursynth\nDescription: VapourSynth frameserver\nVersion: 82\nLibs: -L${{libdir}} -lvsscript -lvapoursynth\nLibs.private: -ldl -lpthread -lstdc++\nCflags: -I${{includedir}}\n",
            e.prefix
        ),
    )?;
    // dist/site-packages 整树删掉——libvapoursynthfilters.so 插件和 .py
    // 打包货静态链一概不需要，留着只会被 ldd 白名单抓到 /
    // drop the dist/site-packages tree entirely — the filters plugin
    // .so and the .py payload are useless to a static chain and only
    // trip the ldd allowlist
    if let Some(py) = drop.parent().and_then(|p| p.parent())
        && py.starts_with(e.prefix_win.join("lib"))
    {
        crate::fsx::rm_rf(py);
    }
    Ok(())
}
