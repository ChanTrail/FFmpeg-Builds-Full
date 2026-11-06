//! pkg-config 查询与 .pc 文件修补（extras-lib.sh 与 deps_fixup 共用件）
//! pkg-config queries and .pc patching (shared by extras-lib.sh & deps_fixup)

use crate::fsx;
use crate::sys;
use std::path::{Path, PathBuf};

/// pkg-config 查询统一入口 / single entry point for pkg-config invocations
pub fn pc(args: &[&str]) -> Result<String, String> {
    sys::capture("pkg-config", args, &sys::env().work)
}

/// pkg-config --exists <mod> / pkg-config --exists <mod>
pub fn exists(name: &str) -> bool {
    pc(&["--exists", name]).is_ok()
}

/// pkg-config --modversion / pkg-config --modversion
pub fn modversion(name: &str) -> String {
    pc(&["--modversion", name])
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// pkg-config --libs / pkg-config --libs
pub fn libs(name: &str) -> String {
    pc(&["--libs", name]).unwrap_or_default().trim().to_string()
}

/// pkg-config --libs --static / pkg-config --libs --static
pub fn libs_static(name: &str) -> Result<String, String> {
    pc(&["--libs", "--static", name])
}

/// pkg-config --variable=<v> / pkg-config --variable=<v>
pub fn variable(name: &str, var: &str) -> String {
    pc(&[&format!("--variable={var}"), name])
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// 单个验收记号：pkg-config 模块 | file:<前缀内路径> | hdrsym:<头@符号> |
/// arsym:<归档@符号> | "-" | "always"（extras_check 的移植）
/// One acceptance token: pkg-config module | file:<path under prefix> |
/// hdrsym:<header@symbol> | arsym:<archive@symbol> | "-" | "always"
pub fn check_token(c: &str) -> Result<(), String> {
    let e = sys::env();
    if c == "-" || c == "always" {
        return Ok(());
    }
    if let Some(rel) = c.strip_prefix("file:") {
        if e.prefix_win.join(rel).exists() {
            return Ok(());
        }
        return Err(format!("check: missing {}/{rel}", e.prefix));
    }
    if let Some(h) = c.strip_prefix("hdrsym:") {
        let (hdr, sym) = h.split_once('@').ok_or("bad hdrsym spec")?;
        if fsx::contains(&e.prefix_win.join("include").join(hdr), sym) {
            return Ok(());
        }
        return Err(format!("check: {sym} not found in {hdr}"));
    }
    if let Some(a) = c.strip_prefix("arsym:") {
        let (ar_path, sym) = a.split_once('@').ok_or("bad arsym spec")?;
        if crate::ar::has_sym(&e.prefix_win.join(ar_path), sym, &e.work) {
            return Ok(());
        }
        return Err(format!("check: {sym} not defined in {ar_path}"));
    }
    if exists(c) {
        Ok(())
    } else {
        Err(format!("check: pkg-config --exists {c} failed"))
    }
}

/// 逗号分隔验收表：全部记号过才算过 / comma-separated spec — all tokens
/// must pass
pub fn check_spec(spec: &str) -> Result<(), String> {
    for c in spec.split(',') {
        check_token(c.trim())?;
    }
    Ok(())
}

/// pc 文件路径：<prefix>/lib/pkgconfig/<name>.pc（POSIX 前缀转 Windows）
/// Path of a .pc file inside the prefix
pub fn pc_file(name: &str) -> PathBuf {
    sys::env()
        .prefix_win
        .join("lib")
        .join("pkgconfig")
        .join(format!("{name}.pc"))
}

/// pc_token_present：<file> 按空白拆词后是否含精确 token
/// Exact-token check against a file's whitespace-split words
pub fn token_present(file: &Path, token: &str) -> bool {
    fsx::read(file).split_whitespace().any(|t| t == token)
}

/// 往 .pc 的某个键行追加 token（无键则新建行；精确去重）
/// Append tokens to a .pc key line (creates the line when absent, dedups)
fn append_key(name: &str, key: &str, tokens: &[&str]) {
    let f = pc_file(name);
    if !f.is_file() {
        return;
    }
    let mut c = fsx::read(&f);
    for t in tokens {
        if token_present(&f, t) {
            continue;
        }
        let line = format!("{key}:");
        if let Some(pos) = c.find(&line) {
            let end = c[pos..].find('\n').map(|i| pos + i).unwrap_or(c.len());
            c.insert_str(end, &format!(" {t}"));
        } else {
            if !c.ends_with('\n') {
                c.push('\n');
            }
            c.push_str(&format!("{key}: {t}\n"));
        }
        crate::log::log(&format!(
            "patched {}: added {}",
            f.file_name().unwrap().to_string_lossy(),
            t
        ));
    }
    let _ = std::fs::write(&f, c);
}

/// pc_libs_private <name> <lib...> → Libs.private 追加
/// Append -l flags to Libs.private
pub fn libs_private(name: &str, libs: &[&str]) {
    append_key(name, "Libs.private", libs);
}

/// pc_cflags_private <name> <flag...> → Cflags.private 追加
/// Append -D flags to Cflags.private
pub fn cflags_private(name: &str, flags: &[&str]) {
    append_key(name, "Cflags.private", flags);
}

/// pc_requires_private <name> <req...> → Requires.private 追加
/// Append package names to Requires.private
pub fn requires_private(name: &str, reqs: &[&str]) {
    append_key(name, "Requires.private", reqs);
}

/// pc_require：依赖真装上了才往 Requires.private 追加（deps_fixup 语义）
/// Append to Requires.private only when the dep is actually installed
pub fn require(name: &str, deps: &[&str]) {
    let f = pc_file(name);
    if !f.is_file() {
        return;
    }
    let avail: Vec<&str> = deps
        .iter()
        .copied()
        .filter(|d| !token_present(&f, d) && exists(d))
        .collect();
    requires_private(name, &avail);
}

/// pc_dedup_fields：合并 .pc 里重复的 "Key: value" 键行（pkg-config 0.29.2
/// 对重复字段 exit(1)）/ merge duplicated keyword lines in every .pc
/// (pkg-config 0.29.2 exits(1) on a repeated field)
pub fn dedup_fields() {
    let dir = sys::env().prefix_win.join("lib").join("pkgconfig");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in rd.flatten() {
        let f = e.path();
        if f.extension().map(|x| x == "pc").unwrap_or(false) {
            dedup_one(&f);
        }
    }
}

/// 单文件键行合并（值拼到首次出现处；k=v 变量行不动）
/// Merge duplicate keyword lines of one file (values fold into first
/// occurrence; k=v variable lines untouched)
fn dedup_one(f: &Path) {
    let c = fsx::read(f);
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    let mut lines: Vec<String> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for line in c.lines() {
        let is_key = line
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic())
            .unwrap_or(false)
            && line[..line.find(':').unwrap_or(line.len())]
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
            && line.contains(':')
            && !line.starts_with(char::is_whitespace);
        let has_eq_before_colon = line
            .find('=')
            .map(|i| i < line.find(':').unwrap_or(usize::MAX))
            .unwrap_or(false);
        if is_key && !has_eq_before_colon {
            let key = &line[..line.find(':').unwrap()];
            if let Some(&first) = seen.get(key) {
                let val = line[line.find(':').unwrap() + 1..].trim();
                if !val.is_empty() {
                    lines[first].push(' ');
                    lines[first].push_str(val);
                }
                continue;
            }
            seen.insert(key.to_string(), lines.len());
            order.push(lines.len());
        }
        order.push(lines.len());
        lines.push(line.to_string());
    }
    let out: Vec<&str> = order.iter().map(|&i| lines[i].as_str()).collect();
    if out.join("\n") + "\n" != c {
        let _ = std::fs::write(f, out.join("\n") + "\n");
    }
}

/// pc_sanitize_requires <pcfile>：丢掉解析不到 .pc 的 Requires 记号，
/// 其归档在时的按 -l 形式补进 Libs.private
/// Drop Requires tokens that resolve to no .pc; dropped tokens whose archive
/// exists are re-emitted as -l flags in Libs.private
pub fn sanitize_requires(f: &Path) {
    if !f.is_file() {
        return;
    }
    let mut c = fsx::read(f);
    for key in ["Requires.private", "Requires"] {
        let Some(pos) = c.find(&format!("{key}:")) else {
            continue;
        };
        let end = c[pos..].find('\n').map(|i| pos + i).unwrap_or(c.len());
        let mods = c[pos + key.len() + 1..end].trim().to_string();
        if mods.is_empty() {
            continue;
        }
        // 全行可解析就跳过 / skip when the whole line resolves
        if exists(&mods) {
            continue;
        }
        let (mut keep, mut add) = (String::new(), String::new());
        let mut prev_op = false;
        for m in mods.split_whitespace() {
            if ["=", "<", ">", "<=", ">=", "!="].contains(&m) {
                prev_op = true;
                continue;
            }
            if prev_op {
                prev_op = false;
                continue;
            }
            if exists(m) {
                keep.push(' ');
                keep.push_str(m);
                continue;
            }
            for cand in [
                m.to_string(),
                m.split('-').next().unwrap_or(m).to_string(),
                m.trim_start_matches("lib").to_string(),
            ] {
                if sys::env()
                    .prefix_win
                    .join("lib")
                    .join(format!("lib{cand}.a"))
                    .is_file()
                {
                    add.push_str(&format!(" -l{cand}"));
                    break;
                }
            }
            crate::log::log(&format!(
                "[extras] pc_sanitize: {}: dropped unresolved requires '{m}'",
                f.file_name().unwrap().to_string_lossy()
            ));
        }
        let keep = keep.trim().to_string();
        let new_line = if keep.is_empty() {
            String::new()
        } else {
            format!("{key}: {keep}")
        };
        c.replace_range(pos..end, &new_line);
        if !add.is_empty() {
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            let _ = std::fs::write(f, &c);
            libs_private(&name, &[add.trim()]);
            c = fsx::read(f);
        }
    }
    let _ = std::fs::write(f, c);
}

/// extras_lto_free <archive>：归档无 GCC LTO 字节码标记
/// Reject archives carrying GCC LTO bytecode markers
pub fn lto_free(a: &Path) -> bool {
    match std::fs::read(a) {
        Ok(b) => {
            let n = b.len();
            let hay = &b[..n.min(b.len())];
            let has = |pat: &[u8]| -> bool { hay.windows(pat.len()).any(|w| w == pat) };
            // '.gnu.lto_' 段名与 '__gnu_lto_' 标记符号 / section + marker
            !has(b"gnu.lto_") && !has(b"gnu_lto")
        }
        Err(_) => true,
    }
}

/// extras_static_libs_ok <check>：每个 pkg-config 模块的每个 -l 旗标都
/// 必须解析到静态归档（前缀内还必须 LTO-free）
/// Every -l flag of every pkg-config module in <check> must resolve to a
/// static archive (LTO-free when inside the prefix)
pub fn static_libs_ok(check: &str) -> Result<(), String> {
    let e = sys::env();
    for c in check.split(',') {
        let c = c.trim();
        if c.is_empty()
            || c == "-"
            || c.starts_with("file:")
            || c.starts_with("hdrsym:")
            || c.starts_with("arsym:")
        {
            continue;
        }
        if !exists(c) {
            return Err(format!("pkg-config --exists {c} failed"));
        }
        let msys = if e.env_prefix.is_empty() {
            e.prefix.clone()
        } else {
            e.env_prefix.clone()
        };
        let mut libdirs = vec![
            variable(c, "libdir"),
            format!("{}/lib", e.env_prefix),
            format!("{}/lib64", e.env_prefix),
            format!("{msys}/lib"),
            format!("{msys}/x86_64-w64-mingw32/lib"),
            format!("{msys}/aarch64-w64-mingw32/lib"),
        ];
        let mut libs_line = libs(c);
        if !format!(" {libs_line} ").contains(" -l") {
            // 无 -l 记号时从 .pc 文本兜底 / .pc fallback when no -l tokens
            let pc = {
                let p = Path::new(&variable(c, "pcfiledir")).join(format!("{c}.pc"));
                if p.is_file() { p } else { pc_file(c) }
            };
            if pc.is_file() {
                libs_line = fsx::read(&pc)
                    .lines()
                    .filter(|l| l.starts_with("Libs"))
                    .map(|l| l.split(':').nth(1).unwrap_or("").to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
        let mut probed = false;
        for tok in libs_line.split_whitespace() {
            if let Some(rest) = tok.strip_prefix("-L") {
                libdirs.push(rest.to_string());
                continue;
            }
            if !tok.starts_with("-l") {
                continue;
            }
            probed = true;
            let ar_name = if let Some(exact) = tok.strip_prefix("-l:") {
                exact.to_string()
            } else {
                format!("lib{}.a", &tok[2..])
            };
            let mut found = false;
            for d in &libdirs {
                let dw = sys::posix_to_win(d, &e.msys2_root);
                let cand = dw.join(&ar_name);
                if cand.is_file() {
                    if !d.starts_with(&e.env_prefix) || lto_free(&cand) {
                        found = true;
                        break;
                    }
                    crate::log::log(&format!(
                        "[extras] check: {ar_name} carries foreign LTO bytecode"
                    ));
                }
            }
            if !found {
                // 工具链搜索路径兜底 / compiler search path fallback
                let d = sys::capture(&e.cc, &["-print-file-name", &ar_name], &e.work)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !d.is_empty() && d != ar_name && Path::new(&d).is_file() {
                    found = true;
                }
            }
            if !found {
                return Err(format!("{c} needs {tok} but no static archive resolves"));
            }
        }
        if !probed {
            return Err(format!("{c}: no -l token probed"));
        }
    }
    Ok(())
}
