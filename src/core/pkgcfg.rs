//! pkg-config 查询与 .pc 文件修补
//! pkg-config queries and .pc patching

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
/// arsym:<归档@符号> | "-" | "always"
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

/// pc_require：依赖真装上了才往 Requires.private 追加
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

/// 取参旗标清单：这些旗标吃掉后一个独立 token 作参数，去重/并入时
/// 必须"旗标+参数"整体行动——把旗标当裸 token 去重会把配对打散
/// （-framework A -framework B → -framework A B，clang 将 B 当输入文件
/// 报 no such file or directory）
/// Flags that consume the following standalone token as their argument —
/// dedup/merge must move them as flag+argument pairs; deduping the flag
/// itself as a bare token breaks the pairing (-framework A -framework B
/// becomes -framework A B and clang reports B as a missing input file)
const PAIR_FLAGS: &[&str] = &[
    "-framework",
    "-weak_framework",
    "-iframework",
    "-F",
    "-isystem",
    "-idirafter",
    "-iquote",
    "-imacros",
    "-include",
    "-isysroot",
    "-iprefix",
    "-iwithprefix",
    "-iwithprefixbefore",
    "-I",
    "-L",
    "-D",
    "-U",
    "-l",
    "-u",
    "-x",
    "-B",
    "-o",
    "-Xlinker",
    "-Xpreprocessor",
    "-Xassembler",
    "-Xclang",
];

/// Requires/Conflicts 关系符 / version operators used by Requires/Conflicts
const REQ_OPS: &[&str] = &["=", "<", ">", "<=", ">=", "!="];

/// 把键行值切成语义单元：取参旗标连参数一个单元；Requires* 键按
/// "mod op ver" 三元组；其余 token 各自成单元
/// Split a key-line value into semantic units: argument-taking flags pair
/// with their argument; Requires* keys form "mod op ver" triples; every
/// other token is its own unit
fn units_for(key: &str, line: &str) -> Vec<String> {
    let val = match line.find(':') {
        Some(i) => &line[i + 1..],
        None => line,
    };
    let toks: Vec<&str> = val.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let req = key.starts_with("Requires") || key == "Conflicts";
    while i < toks.len() {
        if req && i + 2 < toks.len() && REQ_OPS.contains(&toks[i + 1]) {
            // "mod op ver" 三元组整体成单元 / keep "mod op ver" triples whole
            out.push(format!("{} {} {}", toks[i], toks[i + 1], toks[i + 2]));
            i += 3;
        } else if PAIR_FLAGS.contains(&toks[i]) && i + 1 < toks.len() {
            // 旗标+参数一个单元 / flag + its argument as one unit
            out.push(format!("{} {}", toks[i], toks[i + 1]));
            i += 2;
        } else {
            out.push(toks[i].to_string());
            i += 1;
        }
    }
    out
}

/// 周期折叠：旧版 dedup_one 的污染产物是"整个值重复 k 遍"
/// （Name: aom aom、Libs.private: -framework A -framework B -framework A
/// -framework B），找值 token 序列的最小周期保留一期即治愈；健康的
/// "-framework A -framework B" 不是周期的，原样保留
/// Collapse repetition periods: the old dedup_one's damage is always
/// "whole value repeated k times" (Name: aom aom, Libs.private:
/// -framework A -framework B -framework A -framework B) — find the
/// smallest period of the value token sequence and keep one period;
/// a healthy "-framework A -framework B" is not periodic and stays
/// untouched
fn collapse_repeats(line: &str) -> String {
    let (head, val) = match line.find(':') {
        Some(i) => (&line[..=i], line[i + 1..].trim()),
        None => ("", line),
    };
    let toks: Vec<&str> = val.split_whitespace().collect();
    for p in 1..=toks.len() / 2 {
        if toks.len().is_multiple_of(p) && toks.iter().enumerate().all(|(i, t)| *t == toks[i % p]) {
            return format!(
                "{}{}{}",
                head,
                if head.is_empty() { "" } else { " " },
                toks[..p].join(" ")
            );
        }
    }
    line.to_string()
}

/// 逆修被打散的 -framework 配对：-framework 只取一个参数，其后跟裸
/// 单词（不是路径/变量/旗标）的，全是丢了旗标的框架名，各自补回
/// "-framework "——上轮 token 级去重把 "-framework A -framework B"
/// 压成 "-framework A B"，前缀里已存在的这种行据此原路复原
/// Re-pair split -framework sequences: -framework takes exactly one
/// argument; bare words (not paths/vars/flags) following its argument
/// are framework names that lost their flag — give each its own
/// "-framework ". The previous token-level dedup collapsed
/// "-framework A -framework B" into "-framework A B"; this restores
/// such lines already in prefixes
fn repair_frameworks(line: &str) -> String {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        out.push(t.to_string());
        i += 1;
        if t == "-framework" && i < toks.len() {
            // 正式参数 / the real argument
            out.push(toks[i].to_string());
            i += 1;
            // 后续裸词补回旗标 / bare following words get their flag back
            while i < toks.len()
                && toks[i]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                out.push(format!("-framework {}", toks[i]));
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// 发射前治愈键行：先周期折叠（旧 bug 的翻倍值），再 -framework 复原
/// （token 级去重的断配对）；两阶段幂等，健康行原样通过
/// Heal a key line before emit: repetition collapse first (the old bug's
/// doubled values), then -framework re-pairing (the token-level dedup's
/// broken pairings); both idempotent, healthy lines pass through
fn heal_key_line(line: &str) -> String {
    let r = collapse_repeats(line);
    if r.contains("-framework") {
        repair_frameworks(&r)
    } else {
        r
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
                // 按语义单元并入首现行——Libs/Cflags 走旗标+参数对，
                // Requires 走 "mod op ver" 三元组；裸 token 并入会把
                // "-framework B" 拆成挂空旗标、把 ">= 2.0" 的版本约束
                // 丢掉 / fold into the first occurrence in semantic
                // units — flag+argument pairs for Libs/Cflags,
                // "mod op ver" triples for Requires; bare-token merging
                // would strand -framework arguments and drop version
                // constraints like ">= 2.0"
                let mut have = units_for(key, &lines[first]);
                // 并入前先治愈 dup 行——否则 "-framework B C" 里的裸 C
                // 会脱离旗标被并入 / heal the dup line before merging —
                // otherwise a bare C in "-framework B C" would merge in
                // stranded from its flag
                for u in units_for(key, &heal_key_line(line)) {
                    if !have.contains(&u) {
                        have.push(u.clone());
                        lines[first].push(' ');
                        lines[first].push_str(&u);
                    }
                }
                continue;
            }
            seen.insert(key.to_string(), lines.len());
            // 注意：这里不能再 order.push——首现行由下面共用的
            // order.push+lines.push 发射，双推会让每个首现键行输出两份
            // （旧实现正是这样把 .pc 越"修"越重复）/ do NOT order.push
            // here — the first-seen line is emitted by the shared
            // order.push+lines.push below; a double push prints every
            // first key line twice (the old implementation kept making
            // .pc files *more* duplicated this way)
        }
        order.push(lines.len());
        lines.push(if is_key && !has_eq_before_colon {
            heal_key_line(line)
        } else {
            line.to_string()
        });
    }
    let out: Vec<&str> = order.iter().map(|&i| lines[i].as_str()).collect();
    if out.join("\n") + "\n" != c {
        let _ = std::fs::write(f, out.join("\n") + "\n");
        crate::log::log(&format!(
            "[extras] merged duplicate key lines in {}",
            f.file_name().unwrap().to_string_lossy()
        ));
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

/// pc_heal_req_ops <content>：愈合 Requires/Requires.private/Conflicts
/// 行里被剥掉运算符的 `mod ver` 裸对——补回 `>=`。旧版补丁器（Rust
/// 移植前的 sed 时代）曾把 `gobject-2.0 >= 2.56.0` 修成
/// `gobject-2.0 2.56.0`，裸版本 `2.56.0` 随后被 pkg-config 当成
/// 不存在的伪包（"Package '2.56.0', required by 'gdk-pixbuf-2.0',
/// not found"）；坏 .pc 随 deps 快照/环境缓存无限续命，pacman
/// --needed 又不会重写已装文件。msys2 的 Requires 约束一律是 `>=`
/// 写法，恢复是安全的；幂等，健康行原样通过
/// Reinsert `>=` into bare `mod ver` pairs inside
/// Requires/Requires.private/Conflicts lines whose operator was
/// stripped. A pre-Rust sed-era patcher rewrote
/// `gobject-2.0 >= 2.56.0` as `gobject-2.0 2.56.0`, leaving the bare
/// `2.56.0` to be read by pkg-config as a phantom package
/// ("Package '2.56.0', required by 'gdk-pixbuf-2.0', not found"); the
/// damaged .pc files ride the deps snapshot/env cache forever and
/// `pacman --needed` never rewrites installed files. msys2 Requires
/// constraints are uniformly `>=`, so restoring is safe; idempotent,
/// healthy lines pass through unchanged
pub fn heal_req_ops(c: &str) -> String {
    let mut out = String::with_capacity(c.len() + 16);
    for line in c.split_inclusive('\n') {
        let (body, nl) = match line.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (line, ""),
        };
        let (key, val) = match body.split_once(':') {
            Some((k, v)) => (k.trim(), v),
            None => {
                out.push_str(line);
                continue;
            }
        };
        if !matches!(key, "Requires" | "Requires.private" | "Conflicts") {
            out.push_str(line);
            continue;
        }
        let toks: Vec<&str> = val.split_whitespace().collect();
        let mut parts: Vec<&str> = Vec::with_capacity(toks.len() + 2);
        // 前一记号是普通模块名时，紧跟的裸版本说明运算符被剥了 /
        // a bare version right after a plain module name means the
        // operator between them was stripped
        let mut prev_was_name = false;
        for t in toks {
            let core = t.trim_end_matches(',');
            let bare_ver = !core.is_empty()
                && core.chars().next().unwrap().is_ascii_digit()
                && core.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
            let is_op = REQ_OPS.contains(&t);
            if bare_ver && prev_was_name {
                parts.push(">=");
            }
            parts.push(t);
            prev_was_name = !is_op && !bare_ver;
        }
        out.push_str(key);
        out.push(':');
        if !parts.is_empty() {
            out.push(' ');
            out.push_str(&parts.join(" "));
        }
        out.push_str(nl);
    }
    out
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
            if !found && e.os != sys::Os::Windows && links_system(tok) {
                found = true;
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

/// 系统库链接探针：-lm/-lc++/-lz/-ldl/-liconv 这类记号在 POSIX 上指向
/// OS 随带的 dylib/tbd（libSystem），永远不会有 .a 落在搜索路径里。
/// 空程序试链一次判断旗标是否在链接器默认路径上可解析——故意不带
/// .pc 的 -L：指向前缀的 -L 会让 .dylib 蒙混过关（那是运行时依赖而非
/// 静态输入），指向 brew 这类第三方前缀的 -L 同样不是系统库。
/// System-library link probe: tokens like -lm/-lc++/-lz/-ldl/-liconv name
/// OS-provided dylibs/tbd stubs (libSystem) that never ship a .a on the
/// search path. Link an empty program to decide whether the flag resolves
/// on the linker's default paths — deliberately WITHOUT the .pc's -L
/// dirs: a -L into the prefix would let a .dylib slip through (a runtime
/// dep, not a static input), and a -L into a third-party prefix like
/// Homebrew is not a system library either.
fn links_system(tok: &str) -> bool {
    let e = sys::env();
    let probe_c = e.work.join(".ffbuild-pkgprobe.c");
    let probe_o = e.work.join(".ffbuild-pkgprobe");
    if fsx::write(&probe_c, "int main(void){return 0;}\n").is_err() {
        return false;
    }
    let args = [
        "-x",
        "c",
        &probe_c.to_string_lossy(),
        "-o",
        &probe_o.to_string_lossy(),
        tok,
    ];
    let ok = sys::run(&e.cc, &args, &e.work).unwrap_or(1) == 0;
    let _ = std::fs::remove_file(&probe_o);
    ok
}
