//! 日志与 CI 注解（lib.sh 的 log/warn/die/group 移植）
//! Logging and CI annotations (port of lib.sh's log/warn/die/group)

/// 打印构建日志行（[build] 前缀）
/// Print a build log line ([build] prefix)
pub fn log(msg: &str) {
    println!("[build] {msg}");
}

/// 开始 CI 日志折叠组
/// Begin a CI log fold group
pub fn group_begin(title: &str) {
    println!("::group::{title}");
}

/// 结束当前折叠组
/// End the current fold group
pub fn group_end() {
    println!("::endgroup::");
}

/// 发 CI 警告注解
/// Emit a CI warning annotation
pub fn warn(msg: &str) {
    println!("::warning::{msg}");
}

/// 发 CI 错误注解并以退出码 1 结束进程（脚本侧立即终止语义）
/// Emit a CI error annotation and end the process with exit code 1
pub fn die(msg: &str) -> ! {
    eprintln!("::error::{msg}");
    std::process::exit(1);
}
