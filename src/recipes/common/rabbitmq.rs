//! rabbitmq 配方：AMQP 客户端（旧 60-media-io.sh 移植）
//! rabbitmq recipe: AMQP client (port of 60-media-io.sh)

use crate::buildsys;
use crate::extras::{Ctx, Recipe};
use crate::recipes::r;
use crate::sys;

/// 配方表项 / recipe table entry
pub fn recipe() -> Recipe {
    let mut x = r(
        "rabbitmq",
        "linux,mac",
        "librabbitmq",
        "--enable-librabbitmq",
        body,
    );
    x.src = include_str!("rabbitmq.rs");
    x
}

/// 构建 rabbitmq-c / Build rabbitmq-c
fn body(ctx: &Ctx) -> Result<(), String> {
    let e = sys::env();
    let src = ctx.src()?;
    // 只有前缀里真有 OpenSSL 才开 SSL：否则 cmake 退回系统/Homebrew 的
    // openssl、链上并在 librabbitmq.pc 记下 -lssl -lcrypto——消费者解析
    // 成空链 / SSL only when the prefix actually ships OpenSSL: otherwise
    // cmake falls back to a system/Homebrew openssl and records -lssl
    // -lcrypto that resolves to nothing for consumers
    let mut args = vec![
        "-DBUILD_TOOLS=OFF".to_string(),
        "-DBUILD_TOOLS_DOCS=OFF".to_string(),
        "-DBUILD_EXAMPLES=OFF".to_string(),
        "-DBUILD_TESTING=OFF".to_string(),
        "-DBUILD_STATIC_LIBS=ON".to_string(),
        "-DBUILD_SHARED_LIBS=OFF".to_string(),
    ];
    if e.prefix_win.join("lib/libssl.a").is_file() {
        args.push("-DENABLE_SSL_SUPPORT=ON".into());
        args.push(format!("-DOPENSSL_ROOT_DIR={}", e.prefix));
        args.push("-DOPENSSL_USE_STATIC_LIBS=ON".into());
    } else {
        args.push("-DENABLE_SSL_SUPPORT=OFF".into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    buildsys::cmake_build(&src, &refs)
}
