//! alsa 配方：Linux 音频采集
//! alsa recipe: Linux audio capture

use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("alsa", "linux", "alsa", "--enable-alsa", body);
    x.src = include_str!("alsa.rs");
    x
}

/// 构建 alsa-lib 静态库 / Build alsa-lib statically
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // alsa-lib 静态构建不接受 --with-pic；目标文件仍通过全局 CFLAGS 得到
    // PIC，只需去掉 libtool 开关 / alsa-lib refuses --with-pic for a static
    // build; objects are still PIC via global CFLAGS
    sys::run_ok(
        "./configure",
        &[
            &format!("--prefix={}", e.prefix),
            &format!("--libdir={}/lib", e.prefix),
            // alsa.conf 是 libasound 的运行时数据文件，静态链接罩不住它
            // ——默认 $datadir/alsa 把 /opt/ffbuild 烤进二进制，用户机
            // 上没有该路径，snd_config_update_r 报 "Cannot access
            // file /opt/ffbuild/share/alsa/alsa.conf"（实证）。指向
            // 系统标准路径：能跑 ALSA 的机器必然装着这份数据
            // （libasound2-data 与 libasound 本体捆绑），缺它的机器
            // ALSA 本来就不可用。用户仍可用 ALSA_CONFIG_DIR /
            // ALSA_CONFIG_PATH 环境变量覆盖 / alsa.conf is libasound's
            // runtime DATA file — static linking cannot cover it. The
            // default $datadir/alsa bakes /opt/ffbuild into the binary;
            // user machines lack that path and snd_config_update_r
            // reports "Cannot access file
            // /opt/ffbuild/share/alsa/alsa.conf" (empirical). Point it
            // at the system-standard location: any ALSA-capable machine
            // carries this data (libasound2-data ships with libasound
            // itself), and machines without it cannot use ALSA anyway.
            // ALSA_CONFIG_DIR / ALSA_CONFIG_PATH env vars still
            // override
            "--with-configdir=/usr/share/alsa",
            // 外部插件模块（alsa-plugins 的 pulse/jack/a52 等 .so）也是
            // 运行时 dlopen 数据——默认 $libdir/alsa-lib 同样烤前缀。
            // 单目录无回退，取 Debian/Ubuntu multiarch 位（本系 CI 出
            // 身 Ubuntu）；其它发行版用户可 ALSA_PLUGIN_DIR 覆盖。安
            // 装侧无物落此目录（smixer 模块默认关），免 stage /
            // external plugin modules (alsa-plugins' pulse/jack/a52
            // .so's) are likewise dlopen'd runtime data — the default
            // $libdir/alsa-lib bakes the prefix too. Single dir, no
            // fallback, so take the Debian/Ubuntu multiarch slot (this
            // CI lineages Ubuntu); other distros can override via
            // ALSA_PLUGIN_DIR. Nothing installs into it (smixer
            // modules are off by default) — no staging needed
            "--with-plugindir=/usr/lib/x86_64-linux-gnu/alsa-lib",
            "--disable-shared",
            "--enable-static",
            "--disable-python",
            "--disable-topology",
            "--disable-ucm",
            "--disable-alisp",
            "--disable-old-symbols",
            "--disable-seq",
            "--disable-rawmidi",
            "--disable-hwdep",
            "--disable-aload",
        ],
        &src,
    )?;
    let jobs = e.jobs.to_string();
    sys::run_ok("make", &["-j", &jobs], &src)?;
    // alsaconfigdir 同时是编译期查找路径与数据文件安装目录——命令行
    // 覆盖只改安装侧（命令行变量压过各 Makefile 赋值且经 MAKEFLAGS
    // 传进 cards/ctl/pcm 子 make），数据落回前缀，构建机自己的
    // /usr/share/alsa 不被碰 / alsaconfigdir is BOTH the compiled-in
    // lookup path and the data install dir — the command-line override
    // touches only the install side (command-line vars beat Makefile
    // assignments and propagate into the cards/ctl/pcm sub-makes via
    // MAKEFLAGS), landing the data back in the prefix while the build
    // machine's own /usr/share/alsa stays untouched
    sys::run_ok(
        "make",
        &["install", &format!("alsaconfigdir={}/share/alsa", e.prefix)],
        &src,
    )
}
