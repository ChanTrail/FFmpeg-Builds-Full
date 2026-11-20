//! fontconfig 配方：字体枚举/匹配库（macOS 无系统 fontconfig，静态
//! 归档要自造；Windows 侧 MSYS2 那份把 /ucrt64/etc/fonts 烤死——
//! 用户机上无此路径即 FcInit 全灭——同配方顶替，借上游内置的
//! WINDOWSFONTDIR/内置配置兜底直接吃系统字体）
//! fontconfig recipe: font enumeration/matching library (macOS has no
//! system fontconfig — the static archive has to be built here; on
//! Windows the MSYS2 package bakes /ucrt64/etc/fonts, which is absent
//! on user machines and kills FcInit — this recipe replaces it and,
//! via upstream's WINDOWSFONTDIR tokens + built-in fallback config,
//! resolves the OS's own fonts with zero shipped files)

use crate::buildsys;
use crate::extras::{self, Ctx, Recipe};
use crate::recipes::r;
use crate::sys::{self, Os};

/// 配方表项（双条目：POSIX 走 pc 校验，windows 用 .vendored 哨兵——
/// MSYS2 的 fontconfig.pc/归档就在同前缀，provided() 会把平台栈的
/// 那份当成已安装而永远跳过本配方；哨兵是 MSYS2 永远不会产出的
/// 路径，逼着 windows 腿真建；glib 同款先例）/ dual table entries:
/// posix legs check the pc module while windows checks a .vendored
/// sentinel — MSYS2's own fontconfig.pc/.a live in the same prefix,
/// so provided() would treat the platform copy as installed and skip
/// this recipe forever; the sentinel is a path MSYS2 never produces,
/// forcing a real build on windows (same precedent as glib)
pub fn recipes() -> Vec<Recipe> {
    let mut a = r(
        "fontconfig",
        "linux,mac",
        "fontconfig",
        "--enable-libfontconfig",
        body,
    );
    a.src = include_str!("fontconfig.rs");
    let mut b = r(
        "fontconfig",
        "windows",
        "file:lib/.vendored-fontconfig",
        "--enable-libfontconfig",
        body,
    );
    b.src = include_str!("fontconfig.rs");
    vec![a, b]
}

/// 构建 fontconfig / Build fontconfig
fn body(ctx: &Ctx) -> Result<(), String> {
    let src = ctx.src()?;
    let e = sys::env();
    if e.os == Os::Windows {
        // Windows：直接 meson_build——upstream 对 windows 宿主天生
        // 全套正确：default-fonts-dirs 默认 ['yes'] →
        // FC_DEFAULT_FONTS=<dir>WINDOWSFONTDIR</dir><dir>
        // WINDOWSUSERFONTDIR</dir>（fcxml.c 运行时经
        // GetSystemWindowsDirectory/SHGetFolderPath 解析成
        // C:\Windows\Fonts 与 %LOCALAPPDATA%\Microsoft\Windows\
        // Fonts——系统自带字体直接可用）；cache-dir 默认 →
        // LOCAL_APPDATA_FONTCONFIG_CACHE 用户可写；且
        // FcInitLoadOwnConfig 在 fonts.conf 找不到时回落
        // FcInitFallbackConfig 内置配置——前缀烤死的
        // etc/fonts 缺失反而自动让位，零补丁零文件可用。
        // default-fonts-dirs 追加 CUSTOMFONTDIR/APPSHAREFONTDIR
        // 两个 exe 相对令牌：ffmpeg.exe 旁放 fonts\ 或
        // share\fonts\ 的便携字体包也自动入列。MSYS2 那份的
        // 断点（/ucrt64/etc/fonts 烤死 → FcInit 全灭）随本
        // 配方整体顶替。
        // Windows: plain meson_build — upstream is natively right on
        // a windows host: default-fonts-dirs ['yes'] expands
        // FC_DEFAULT_FONTS to <dir>WINDOWSFONTDIR</dir><dir>
        // WINDOWSUSERFONTDIR</dir> (fcxml.c resolves them at runtime
        // via GetSystemWindowsDirectory/SHGetFolderPath to
        // C:\Windows\Fonts and %LOCALAPPDATA%\Microsoft\Windows\
        // Fonts — the built-in fonts work directly); cache-dir
        // defaults to the user-writable LOCAL_APPDATA_FONTCONFIG_
        // CACHE; and FcInitLoadOwnConfig falls back to
        // FcInitFallbackConfig's built-in config when fonts.conf is
        // missing — the dead prefix-baked etc/fonts path yields
        // automatically. No patch, no files needed. Appending
        // CUSTOMFONTDIR/APPSHAREFONTDIR adds the two exe-relative
        // tokens so a portable fonts\ or share\fonts\ dir next to
        // ffmpeg.exe joins the set too. The MSYS2 build's break
        // (/ucrt64/etc/fonts baked dead → FcInit fails) is replaced
        // wholesale by this recipe.
        let vend_stamp = ctx.stamp();
        buildsys::meson_build(
            &src,
            &[
                "-Ddoc=disabled",
                "-Dtests=disabled",
                "-Dtools=disabled",
                "-Dcache-build=disabled",
                "-Dnls=disabled",
                "-Dxml-backend=libxml2",
                "-Ddefault-fonts-dirs=WINDOWSFONTDIR,WINDOWSUSERFONTDIR,CUSTOMFONTDIR,APPSHAREFONTDIR",
                "--wrap-mode=nofallback",
            ],
        )?;
        // 哨兵兼任 vendored 清单（glib 同款）：同名 .a/.pc 覆盖进
        // pacman 共树前缀，哨兵让 windows 表项认出"自己建的"，清单
        // 豁免 pacman 排除规则让产物随 overlay 持久化 / the
        // sentinel doubles as the vendored manifest (glib precedent):
        // same-named .a/.pc land in the pacman-shared prefix — the
        // sentinel lets the windows entry recognise its own build and
        // the manifest exempts these paths from pacman exclusion so
        // they persist in the overlay
        let t = std::fs::metadata(&vend_stamp)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::now());
        extras::vendored_manifest("lib/.vendored-fontconfig", t)?;
        return Ok(());
    }
    // 运行期配置目录按 OS 取系统标准位：Linux 全系发行版是
    // /etc/fonts（fontconfig-config 提供）；macOS arm64 以 Homebrew
    // 约定 /opt/homebrew/etc/fonts（brew 的 fontconfig 装那——无
    // brew fontconfig 的机器与现状一样拿不到配置，但没有它
    // fontconfig 本就无法工作；env FONTCONFIG_PATH/FONTCONFIG_FILE
    // 仍可覆盖）/ pick the OS-standard runtime config dir: /etc/fonts
    // on every Linux distro (fontconfig-config ships it);
    // /opt/homebrew/etc/fonts on macOS arm64 per Homebrew convention
    // (brew's fontconfig lives there — a machine without brew
    // fontconfig gets no config either way, but fontconfig cannot work
    // anyway then; FONTCONFIG_PATH/FONTCONFIG_FILE env still override)
    let (basecfg, cfgdir, cachedir) = if e.os == Os::Mac {
        (
            "/opt/homebrew/etc/fonts",
            "/opt/homebrew/etc/fonts/conf.d",
            "/opt/homebrew/var/cache/fontconfig",
        )
    } else {
        ("/etc/fonts", "/etc/fonts/conf.d", "/var/cache/fontconfig")
    };
    // 前缀缺 fontconfig 时 pango 的 meson 自动探测跳过 fontconfig 后端
    // → 不产 pangoft2 → librsvg 的 rsvg-convert 无条件 extern
    // `#[cfg(unix)] FcGetVersion`（上游假定 unix 必有 fontconfig；
    // pangoft2_dep 在 darwin 仅 optional）在最终链接报
    // Undefined symbol。装进前缀后链全通：
    //   fontconfig.pc → pango 建 pangoft2 → pangoft2.pc 的
    //   Libs.private 带 -lfontconfig -lfreetype → cargo 链接解析
    //   FcGetVersion；顺带 FFmpeg 的 --enable-libfontconfig 探测
    //   （features.rs:124 exists("fontconfig")）也不再静默降级。
    // without fontconfig in the prefix, pango's meson auto-detect skips
    // the fontconfig backend → no pangoft2 → librsvg's rsvg-convert,
    // which unconditionally externs `#[cfg(unix)] FcGetVersion`
    // (upstream assumes every unix has fontconfig; pangoft2_dep is only
    // optional on darwin), dies at link with an undefined symbol.
    // Installed into the prefix the whole chain links through:
    //   fontconfig.pc → pango builds pangoft2 → pangoft2.pc's
    //   Libs.private carries -lfontconfig -lfreetype → cargo resolves
    //   FcGetVersion; FFmpeg's --enable-libfontconfig probe
    //   (features.rs:124 exists("fontconfig")) also stops degrading.
    //
    // -Dxml-backend=libxml2：auto 会先探 expat——命中 SDK 的
    // libexpat.tbd 就把一份系统动态库依赖缝进静态链；显式用前缀自造
    // 的静态 libxml2 / auto probes expat first — hitting the SDK's
    // libexpat.tbd would sew a system dylib dependency into the static
    // chain; pin the prefix's own static libxml2 instead
    //
    // fonts.conf 与字体缓存是 fontconfig 的运行时数据/目录，静态链
    // 接罩不住——默认 SYSCONFDIR/fonts 把 $prefix/etc/fonts 烤进
    // 二进制，用户机上没有 /opt/ffbuild，FcInit 直接失败（与
    // alsa.conf 同病）。default-fonts-dirs 默认已是各 OS 的系统字
    // 体目录，无需动 / fonts.conf and the font cache are fontconfig's
    // runtime data/dir — static linking cannot cover them. The default
    // SYSCONFDIR/fonts bakes $prefix/etc/fonts into the binary; user
    // machines lack /opt/ffbuild and FcInit fails outright (same
    // disease as alsa.conf). default-fonts-dirs already defaults to the
    // OS's system font dirs — leave it
    //
    // meson_build_destdir：*-dir 选项既当编译期路径又当安装目录——
    // 直装会把文件写进构建机的真 /etc/fonts、/var/cache，经 stage
    // 中转后只把 $prefix 子树合回前缀，系统目录那份丢弃 /
    // meson_build_destdir: the *-dir options double as compile-time
    // paths and install dirs — a direct install would write into the
    // build machine's real /etc/fonts and /var/cache; staging drops
    // that system-dir copy and merges only the $prefix subtree
    buildsys::meson_build_destdir(
        &src,
        &[
            "-Ddoc=disabled",
            "-Dtests=disabled",
            "-Dtools=disabled",
            "-Dcache-build=disabled",
            "-Dnls=disabled",
            "-Dxml-backend=libxml2",
            &format!("-Dbaseconfig-dir={basecfg}"),
            &format!("-Dconfig-dir={cfgdir}"),
            &format!("-Dcache-dir={cachedir}"),
            "--wrap-mode=nofallback",
        ],
        &src.join("fcstage"),
    )
}
