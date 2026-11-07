//! 文件系统助手：递归遍历、就地替换、find -newer、install
//! Filesystem helpers: recursive walk, in-place replace, find -newer, install

use std::fs;
use std::path::{Path, PathBuf};

/// 递归收集目录下的文件（不含目录本身；跟随符号链接由调用方决定——
/// 这里不跟随，与 find 的默认行为一致）
/// Recursively collect files under dir (directories excluded; symlinks not
/// followed — find's default behaviour)
pub fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 读文本文件（UTF-8 损失容忍）/ Read a text file (lossy UTF-8)
pub fn read(p: &Path) -> String {
    fs::read(p)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// 写文本文件（自动建父目录）/ Write a text file (parents created)
pub fn write(p: &Path, s: &str) -> Result<(), String> {
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {d:?}: {e}"))?;
    }
    fs::write(p, s).map_err(|e| format!("write {p:?}: {e}"))
}

/// 追加一行 / Append one line
pub fn append_line(p: &Path, line: &str) -> Result<(), String> {
    use std::io::Write;
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {d:?}: {e}"))?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .map_err(|e| format!("open {p:?}: {e}"))?;
    writeln!(f, "{line}").map_err(|e| format!("write {p:?}: {e}"))
}

/// 文件是否包含子串 / Whether a file contains a substring
pub fn contains(p: &Path, needle: &str) -> bool {
    read(p).contains(needle)
}

/// grep -rl <needle> <dir>：返回内容含任一 needle 的文件
/// grep -rl <needle> <dir>: files containing any of the needles
pub fn grep_l(dir: &Path, needles: &[&str]) -> Vec<PathBuf> {
    walk(dir)
        .into_iter()
        .filter(|f| {
            let c = read(f);
            needles.iter().any(|n| c.contains(n))
        })
        .collect()
}

/// 原地子串替换（s/old/new/ 全文）/ In-place substring replace (global)
pub fn replace_in(p: &Path, from: &str, to: &str) -> Result<(), String> {
    let c = read(p);
    let n = c.replace(from, to);
    if n == c {
        return Ok(());
    }
    fs::write(p, n).map_err(|e| format!("write {p:?}: {e}"))
}

/// 就地 sed（GNU/BSD 通用 -i.bak 语义；调真实 sed 保证正则语义一致）
/// In-place sed (GNU/BSD -i.bak semantics; calls real sed for regex parity)
pub fn sed_i(expr: &str, file: &Path, dir: &Path) -> Result<(), String> {
    let f = file.to_string_lossy().to_string();
    crate::sys::run_ok("sed", &["-i.bak", expr, &f], dir)?;
    let _ = fs::remove_file(format!("{f}.bak"));
    Ok(())
}

/// 就地 sed -E（ERE 变体；(...) 分组、| 选择、? 量词裸写，GNU/BSD 通用）
/// In-place sed -E (ERE variant; bare (...) groups, | alternation,
/// ? quantifier, GNU/BSD portable)
pub fn sed_i_ere(expr: &str, file: &Path, dir: &Path) -> Result<(), String> {
    let f = file.to_string_lossy().to_string();
    crate::sys::run_ok("sed", &["-i.bak", "-E", expr, &f], dir)?;
    let _ = fs::remove_file(format!("{f}.bak"));
    Ok(())
}

/// find <dir> -newer <stamp>：输出 POSIX 相对路径（./x 形式，按 C 序排）
/// find <dir> -newer <stamp>: POSIX relative paths (./x form, C-sorted)
pub fn find_newer(dir: &Path, stamp_mtime: std::time::SystemTime) -> Vec<String> {
    walk(dir)
        .into_iter()
        .filter(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .map(|t| t > stamp_mtime)
                .unwrap_or(false)
        })
        .filter_map(|p| {
            p.strip_prefix(dir)
                .ok()
                .map(|r| format!("./{}", r.to_string_lossy().replace('\\', "/")))
        })
        .collect()
}

/// 目录清单（./x 形式，文件与符号链接都算）/ dir listing in ./x form
pub fn manifest(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else {
                // ft.is_file() || ft.is_symlink() —— 都收 / collect both
                if let Ok(r) = p.strip_prefix(dir) {
                    out.push(format!("./{}", r.to_string_lossy().replace('\\', "/")));
                }
            }
        }
    }
    out.sort();
    out
}

/// install -d：创建目录 / install -d
pub fn install_d(d: &Path) -> Result<(), String> {
    fs::create_dir_all(d).map_err(|e| format!("mkdir {d:?}: {e}"))
}

/// install -m644：复制文件到目标（Windows 上模式位无关紧要）
/// install -m644: copy a file into place (mode bits are moot on Windows)
pub fn install(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(d) = dst.parent() {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {d:?}: {e}"))?;
    }
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("cp {src:?} -> {dst:?}: {e}"))
}

/// rm -rf 容错版 / tolerant rm -rf
pub fn rm_rf(p: &Path) {
    if p.is_dir() {
        let _ = fs::remove_dir_all(p);
    } else {
        let _ = fs::remove_file(p);
    }
}

/// 删除 dir 下匹配通配模式的条目（只支持 *，与 shell glob 等价）
/// Remove entries under dir matching a shell-style glob (* wildcard only)
pub fn rm_glob(dir: &Path, pattern: &str) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if glob_match(pattern, &name) {
            rm_rf(&e.path());
        }
    }
}

/// 简单通配匹配（* = 任意序列，其他字符逐字比较）
/// Simple glob match (* = any sequence, other chars literal)
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let mut pi = pattern.chars().peekable();
    let mut ni = name.chars().peekable();
    // 回溯点（上一个 * 的位置）/ backtrack point (last * position)
    let (mut star_p, mut star_n) = (usize::MAX, 0usize);
    let pn: Vec<char> = pattern.chars().collect();
    let nn: Vec<char> = name.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    while j < nn.len() {
        if i < pn.len() && (pn[i] == '?' || pn[i] == nn[j]) {
            i += 1;
            j += 1;
        } else if i < pn.len() && pn[i] == '*' {
            star_p = i;
            star_n = j;
            i += 1;
        } else if star_p != usize::MAX {
            i = star_p + 1;
            star_n += 1;
            j = star_n;
        } else {
            return false;
        }
    }
    while i < pn.len() && pn[i] == '*' {
        i += 1;
    }
    let _ = (&mut pi, &mut ni);
    i == pn.len()
}

/// 追加原始文本到文件末尾（不补换行；自动建父目录）
/// Append raw text to a file (no newline added; parents created)
pub fn append(p: &Path, s: &str) -> Result<(), String> {
    use std::io::Write;
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| format!("mkdir {d:?}: {e}"))?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .map_err(|e| format!("open {p:?}: {e}"))?;
    f.write_all(s.as_bytes())
        .map_err(|e| format!("write {p:?}: {e}"))
}
