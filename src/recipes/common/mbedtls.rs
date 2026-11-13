//! mbedtls 配方：轻量 TLS/crypto 栈（libssh 的加密后端——openssl 在 linux/mac 上都是动态库不能链）
//! mbedtls recipe: the lightweight TLS/crypto stack (libssh's crypto backend — openssl is dynamic on linux/mac and can't be linked)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r("mbedtls", "linux,mac", "file:lib/libmbedtls.a", "-", body);
    x.src = include_str!("mbedtls.rs");
    x
}

/// 构建 mbedtls / Build mbedtls
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = crate::sys::env();
    let src = ctx.src()?;
    // libssh 的 crypto 线程实现（threads/mbedtls.c）要求 mbedtls 开出
    // THREADING_C+THREADING_PTHREAD——默认配置两项都注释着，缺了
    // crypto_thread_init 会撞 #warn 编译错；按行解注，其余配置不动 /
    // libssh's crypto thread impl (threads/mbedtls.c) requires
    // MBEDTLS_THREADING_C + MBEDTLS_THREADING_PTHREAD — both are
    // commented out in the default config and crypto_thread_init dies
    // on its #warn; uncomment just those two lines, leave the rest alone
    let cfg = src.join("include/mbedtls/mbedtls_config.h");
    let c = crate::fsx::read(&cfg);
    let mut out = String::with_capacity(c.len());
    for l in c.lines() {
        if l.starts_with("//#define MBEDTLS_THREADING_C")
            || l.starts_with("//#define MBEDTLS_THREADING_PTHREAD")
        {
            out.push_str(&l.replacen("//", "", 1));
            out.push('\n');
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    if out != c {
        crate::fsx::write(&cfg, &out)?;
    }
    buildsys::cmake_build(
        &src,
        &[
            "-DENABLE_TESTING=OFF",
            "-DENABLE_PROGRAMS=OFF",
            "-DUSE_SHARED_MBEDTLS_LIBRARY=OFF",
            "-DUSE_STATIC_MBEDTLS_LIBRARY=ON",
            "-DMBEDTLS_FATAL_WARNINGS=OFF",
        ],
    )?;
    // THREADING_PTHREAD 让 libmbedcrypto.a 带上 pthread_* 引用——消费方
    // 静态链需要 -lpthread 兜底 / THREADING_PTHREAD leaves pthread_*
    // references inside libmbedcrypto.a — static consumers need
    // -lpthread on the line
    if e.prefix_win.join("lib/pkgconfig/mbedcrypto.pc").is_file() {
        crate::pkgcfg::libs_private("mbedcrypto", &["-lpthread"]);
    }
    Ok(())
}
