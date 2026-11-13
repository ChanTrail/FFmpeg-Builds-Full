//! glib 配方：lensfun、pango、cairo-gobject 依赖（macOS 构建 +
//! Windows vendored 覆盖）
//! glib recipe: dependency of lensfun/pango/cairo-gobject (macOS build +
//! Windows vendored overlay)

use crate::buildsys;
use crate::extras::{self, Ctx, Recipe};
use crate::fsx;
use crate::recipes::r;
use crate::sys::{self, Os};

/// 配方表项（macOS 与 Windows 各一条，共享同一构建体）
/// recipe table entries (one each for macOS and Windows, sharing the
/// same build body)
pub fn recipes() -> Vec<Recipe> {
    let mut a = r(
        "glib",
        "linux,mac",
        "glib-2.0,gobject-2.0,gio-2.0",
        "-",
        body,
    );
    a.src = include_str!("glib.rs");
    let mut b = r(
        "glib",
        "windows",
        "file:lib/.vendored-glib-2.88.4",
        "-",
        body,
    );
    b.src = include_str!("glib.rs");
    vec![a, b]
}

/// 构建 glib-2.0 / Build glib-2.0
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    // Windows：tarball 里 COPYING→LICENSES/LGPL-2.1-or-later.txt 是符号链接，
    // MSYS2 tar 无权限创建 native symlink 而中止；winsymlinks:lnk 落成
    // .lnk 替身文件 / the tarball's COPYING symlink aborts MSYS2 tar (native
    // symlinks need admin); winsymlinks:lnk drops .lnk stand-ins
    let _msys = if e.os == Os::Windows {
        let cur = std::env::var("MSYS").unwrap_or_default();
        Some(sys::EnvGuard::set(&[(
            "MSYS",
            format!(
                "{cur}{}winsymlinks:lnk",
                if cur.is_empty() { "" } else { " " }
            ),
        )]))
    } else {
        None
    };
    let src = ctx.src()?;
    drop(_msys);
    // 归档里 subprojects/gvdb/ 是空壳 gitlink——Meson 当 vendored 项目找
    // meson.build 中止。删掉让 gvdb.wrap 拉真身 / the archive ships
    // subprojects/gvdb/ as an empty gitlink stub — Meson sees it as
    // vendored and aborts; remove it so gvdb.wrap fetches the real thing
    let gvdb = src.join("subprojects/gvdb");
    if gvdb.is_dir() && !gvdb.join("meson.build").is_file() {
        fsx::rm_rf(&gvdb);
    }
    if e.os == Os::Windows {
        // Windows appinfo：resolve_string() 把 @{...} 交给
        // SHLoadIndirectString → MRT 栈会在 runner 某些 UWP 注册表项上跳
        // 堆地址执行。换成 == 让 @{...} 按原样提前返回
        // resolve_string() hands @{...} to SHLoadIndirectString whose MRT
        // stack jumps into a heap address on some runner UWP entries.
        // == makes @{...} strings return early as-is
        fsx::sed_i(
            "s|at_string\\[0\\] != L'@'|at_string[0] == L'@'|",
            &src.join("gio/gwin32appinfo.c"),
            &src,
        )?;
        // 斩草除根：上面的补丁只堵 @{...} 一条路径，而 runner 的 MRT
        // 栈在 grab_registry_string 里仍跳堆地址（0xfb42a66 实证——
        // vendored glib 补丁是否随 deps 世系生效存疑）。ffmpeg 用不到
        // GAppInfo，让 update_registry_data 整个空转：线程照起但零
        // 注册表读、计数照常归零（do_wait 不悬）、API 返回空表。签名
        // 行换成"stub + 改名真身"，原函数体完整保留为 dead function /
        // eradicate the class: the patch above only covers the @{...}
        // path, yet the runner's MRT stack still jumps to a heap
        // address inside grab_registry_string (0xfb42a66 empirical —
        // whether the vendored glib patch survives through the deps
        // lineage is doubtful). ffmpeg needs no GAppInfo, so
        // update_registry_data is stubbed to a no-op: the thread still
        // spawns but reads no registry, the counter still drains
        // (do_wait never hangs) and the API returns empty tables.
        // The signature line becomes "stub + renamed real function";
        // the original body survives intact as a dead function
        fsx::sed_i(
            "s|^update_registry_data (void)$|update_registry_data (void) { return; }\\nstatic void update_registry_data_disabled (void)|",
            &src.join("gio/gwin32appinfo.c"),
            &src,
        )?;
        // ABI 兼容 shim：MSYS2 预编译 librsvg-2.a 按 glib 2.90 链，引用
        // 2.89 才有的 g_notification_backend_dup_application——2641 个
        // glib 未决符号里唯一真缺的。2.88 里 backend->application 是借用
        // 指针，包一层 g_object_ref / MSYS2's prebuilt librsvg-2.a was
        // linked against glib 2.90 and references this 2.89-only getter —
        // the only truly missing symbol of rsvg's 2641 undefined glib
        // symbols. backend->application is a borrowed pointer in 2.88, so
        // wrapping it in g_object_ref suffices
        fsx::sed_i(
            "/g_notification_backend_new_default/a\\\nGApplication *          g_notification_backend_dup_application           (GNotificationBackend *self);",
            &src.join("gio/gnotificationbackend.h"),
            &src,
        )?;
        fsx::append(
            &src.join("gio/gnotificationbackend.c"),
            "\n/* FFmpeg-Builds-Full compat shim — see recipe glib. */\nGApplication *\ng_notification_backend_dup_application (GNotificationBackend *self)\n{\n  g_return_val_if_fail (G_IS_NOTIFICATION_BACKEND (self), NULL);\n  return self->application != NULL ? g_object_ref (self->application) : NULL;\n}\n",
        )?;
    }
    // 没有 'libffi' 选项（硬依赖，会被自动找到）；Meson 遇未知选项中止
    // no 'libffi' option (hard dependency, found automatically); Meson
    // aborts on unknown options
    let vend_stamp = if e.os == Os::Windows {
        Some(ctx.stamp())
    } else {
        None
    };
    buildsys::meson_build(
        &src,
        &[
            "-Dtests=false",
            "-Dintrospection=disabled",
            "-Dlibmount=disabled",
            "-Dselinux=disabled",
            "-Dsysprof=disabled",
            "-Dlibelf=disabled",
        ],
    )?;
    // Windows 哨兵兼任 vendored 清单：$PREFIX 就是 MSYS2 前缀，同名
    // .a/.pc 被本配方覆盖，普通检查分不清谁建的——哨兵让 windows 那条表
    // 项判定 "provided"；清单豁免 pacman 排除规则，产物才随 overlay 持久化
    // the sentinel doubles as the vendored manifest: $PREFIX IS the MSYS2
    // prefix so identical .a/.pc are overwritten in place — the sentinel
    // lets the windows table entry decide "provided", and the manifest
    // exempts these paths from the pacman exclusion so our build persists
    if let Some(stamp) = vend_stamp {
        let t = std::fs::metadata(&stamp)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::now());
        extras::vendored_manifest("lib/.vendored-glib-2.88.4", t)?;
    }
    Ok(())
}
