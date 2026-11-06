//! 静态归档符号操作：合并成员、改名去重、成员删改（deps.sh 移植）
//! Static-archive symbol surgery: member merge, rename-with-dedup,
//! member delete/replace (ported from deps.sh)

use crate::sys;
use std::collections::BTreeSet;
use std::path::Path;

/// ar members <lib.a>：成员名表 / ar members <lib.a>
pub fn members(lib: &Path, dir: &Path) -> Vec<String> {
    sys::capture("ar", &["t", &lib.to_string_lossy()], dir)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// nm 定义符号表（T/t/D/d/R/r/B/b/C 行，去重）
/// nm's defined-symbol table (T/t/D/d/R/r/B/b/C lines, deduped)
fn defined_syms(lib: &Path, dir: &Path) -> BTreeSet<String> {
    let out =
        sys::capture("nm", &["--defined-only", &lib.to_string_lossy()], dir).unwrap_or_default();
    out.lines()
        .filter(|l| {
            l.split_whitespace()
                .nth(1)
                .map(|t| "TtDdRrBbC".contains(t))
                .unwrap_or(false)
        })
        .filter_map(|l| l.split_whitespace().nth(2))
        .map(String::from)
        .collect()
}

/// ar_extract_members <dst> <src.a> <member...>：抽出指定成员
/// Extract listed members from src.a into dst
fn extract(dst: &Path, src: &Path, names: &[String], _dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("mkdir {dst:?}: {e}"))?;
    let mut args: Vec<String> = vec!["x".into(), src.to_string_lossy().into()];
    args.extend(names.iter().cloned());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    sys::run_ok("ar", &refs, dst)
}

/// deps_ar_merge_members <dst.a> <src.a>：符号级去重后按成员并入 dst.a
/// （obj 级整体重复跳过；partial-overlap 也整体跳过——成员互相定义是常态，
/// 硬拆只会让成员间交叉引用断链）
/// Merge src.a members into dst.a after symbol-level dedup (whole archive
/// skipped on full overlap; partial overlap also skipped — members commonly
/// co-define symbols)
pub fn merge_members(dst: &Path, src: &Path, dir: &Path) -> Result<(), String> {
    if !src.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {dir:?}: {e}"))?;
    let m: Vec<String> = members(src, dir)
        .into_iter()
        .filter(|n| n != "/" && n != "//" && n != "__.SYMDEF")
        .collect();
    if m.is_empty() {
        return Ok(());
    }
    let src_syms = defined_syms(src, dir);
    let dst_syms = defined_syms(dst, dir);
    if !src_syms.is_empty() && src_syms.is_subset(&dst_syms) {
        crate::log::log(&format!(
            "[deps] ar_merge_members: {} fully overlaps - skip",
            src.file_name().unwrap().to_string_lossy()
        ));
        return Ok(());
    }
    let need = if !dst_syms.is_empty() && src_syms.iter().any(|s| dst_syms.contains(s)) {
        crate::log::log(&format!(
            "[deps] ar_merge_members: {} partial-overlap - skip",
            src.file_name().unwrap().to_string_lossy()
        ));
        return Ok(());
    } else {
        m
    };
    let tmp = dir.join(format!("armerge{}", std::process::id()));
    extract(&tmp, src, &need, dir)?;
    let mut objs: Vec<String> = Vec::new();
    for en in std::fs::read_dir(&tmp)
        .map_err(|e| format!("ls {tmp:?}: {e}"))?
        .flatten()
    {
        objs.push(en.file_name().to_string_lossy().to_string());
    }
    // 逐成员 ar r：同名成员会被换掉而非累加 / ar r per member replaces
    // same-named members instead of appending
    for chunk in objs.chunks(20) {
        let mut args: Vec<String> = vec!["r".into(), dst.to_string_lossy().into()];
        args.extend(chunk.iter().cloned());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        sys::run_ok("ar", &refs, &tmp)?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// deps_ar_rename_obj <dst.a> <src.a>：src.a 成员加 "d_" 前缀后并入 dst.a
/// （符号仍冲突时放弃整个 src.a —— 比如 av1 的 cdef 同名例程）
/// Prepend "d_" to every member of src.a and merge into dst.a (whole src.a
/// dropped when symbols still collide — e.g. av1's cdef routines)
pub fn rename_obj(dst: &Path, src: &Path, dir: &Path) -> Result<(), String> {
    if !src.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {dir:?}: {e}"))?;
    let m: Vec<String> = members(src, dir)
        .into_iter()
        .filter(|n| n != "/" && n != "//" && n != "__.SYMDEF")
        .collect();
    if m.is_empty() {
        return Ok(());
    }
    let src_syms = defined_syms(src, dir);
    let dst_syms = defined_syms(dst, dir);
    if !src_syms.is_empty() && src_syms.is_subset(&dst_syms) {
        crate::log::log(&format!(
            "[deps] ar_rename_obj: {} fully overlaps - skip",
            src.file_name().unwrap().to_string_lossy()
        ));
        return Ok(());
    }
    if !dst_syms.is_empty() && src_syms.iter().any(|s| dst_syms.contains(s)) {
        crate::log::log(&format!(
            "[deps] ar_rename_obj: {} partial-overlap - skip",
            src.file_name().unwrap().to_string_lossy()
        ));
        return Ok(());
    }
    let tmp = dir.join(format!("arrename{}", std::process::id()));
    extract(&tmp, src, &m, dir)?;
    // ar 成员名限 15 字节：d_ 前缀后按序数重命名 / ar member names cap at
    // 15 bytes: rename to d_00001.o ... after prefixing
    let mut objs: Vec<String> = Vec::new();
    for en in std::fs::read_dir(&tmp)
        .map_err(|e| format!("ls {tmp:?}: {e}"))?
        .flatten()
    {
        let n = en.file_name().to_string_lossy().to_string();
        let ext = Path::new(&n)
            .extension()
            .map(|x| format!(".{x}", x = x.to_string_lossy()))
            .unwrap_or_default();
        let dst_name = format!("d_{n}");
        let final_name = if dst_name.len() > 15 {
            format!("d_{:05}{ext}", objs.len() + 1)
        } else {
            dst_name
        };
        std::fs::rename(en.path(), tmp.join(&final_name)).map_err(|e| format!("mv: {e}"))?;
        objs.push(final_name);
    }
    for chunk in objs.chunks(20) {
        let mut args: Vec<String> = vec!["r".into(), dst.to_string_lossy().into()];
        args.extend(chunk.iter().cloned());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        sys::run_ok("ar", &refs, &tmp)?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// ar 成员名转 ASCII 安全名（Windows 文件名不能有 '*'）
/// Map member names to filesystem-safe names (no '*' on Windows)
pub fn member_file(m: &str) -> String {
    member_to_file(m)
}

/// `ar p` 倒出成员字节 / `ar p` — dump a member's raw bytes
pub fn member_data(lib: &Path, member: &str, dir: &Path) -> Result<Vec<u8>, String> {
    sys::capture_bytes("ar", &["p", &lib.to_string_lossy(), member], dir)
}

/// ar 成员名转 ASCII 安全名（Windows 文件名不能有 '*'）
/// Map member names to filesystem-safe names (no '*' on Windows)
fn member_to_file(m: &str) -> String {
    let mut s: String = m
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.len() > 15 {
        let ext = Path::new(&s)
            .extension()
            .map(|x| format!(".{x}", x = x.to_string_lossy()))
            .unwrap_or_default();
        s = format!("{}{}", &s[..14 - ext.len()], ext);
    }
    s
}

/// deps_ar_replace_member <lib.a> <member> <obj>：删老成员、抽出来验证
/// objcopy、再 r 进去（obj 在 cwd）
/// Delete member, extract for an objcopy check, then r the new obj in
pub fn replace_member(lib: &Path, member: &str, obj: &Path, dir: &Path) -> Result<(), String> {
    // 校验新成员：objcopy 可重定位读入即视为合法 COFF
    // validate the new member: objcopy reading it as relocatable suffices
    let chk = dir.join(format!("{}.chk", member_to_file(member)));
    if sys::run(
        "objcopy",
        &[&obj.to_string_lossy(), &chk.to_string_lossy()],
        dir,
    )? != 0
    {
        return Err(format!("ar_replace_member: {obj:?} failed objcopy"));
    }
    let _ = std::fs::remove_file(&chk);
    // 原成员名 >15 字节时 ar 自己截断，照它的截断名 r
    // member names >15 bytes get truncated by ar itself; r under that name
    let name = member.get(..15).unwrap_or(member).to_string();
    let _ = sys::run("ar", &["d", &lib.to_string_lossy(), &name], dir);
    let staged = dir.join(&name);
    std::fs::copy(obj, &staged).map_err(|e| format!("cp {obj:?}: {e}"))?;
    sys::run_ok("ar", &["r", &lib.to_string_lossy(), &name], dir)
}

/// deps_ar_drop_syms <ar> <file-of-syms>：从归档删掉列出的成员
/// Delete members listed in the file from the archive
pub fn drop_syms(ar_path: &Path, list: &Path, dir: &Path) -> Result<(), String> {
    let mut drop = crate::fsx::read(list);
    if drop.trim().is_empty() {
        drop = "dummydummy".into();
    }
    let mut cmd: Vec<String> = vec!["d".into(), ar_path.to_string_lossy().to_string()];
    cmd.extend(drop.split_whitespace().map(String::from));
    let refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
    let _ = sys::run("ar", &refs, dir)?;
    Ok(())
}

/// ar_ensure_sym <lib.a> <sym> [guard...]：把 lib 里定义 sym 的成员抽出来
/// 改成 guard 后缀再 r 回去（libavcodec 的 .text 重定位覆盖修复）
/// Extract members defining <sym>, rename to <guard> and r them back
/// (libavcodec .text relocation overwrite fix)
pub fn ensure_sym(lib: &Path, sym: &str, guard: &str, dir: &Path) -> Result<(), String> {
    let tmp = dir.join(format!("ares{}", std::process::id()));
    for m in members(lib, dir) {
        if m == "/" || m == "//" || m == "__.SYMDEF" {
            continue;
        }
        // 先抽出成员判定其定义符号，命中则删旧名、按 guard 名放回去
        // extract member first; on a hit delete the old name and re-add it
        // under the guard name
        std::fs::create_dir_all(&tmp).ok();
        if extract(&tmp, lib, std::slice::from_ref(&m), dir).is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
            continue;
        }
        let obj = tmp.join(member_to_file(&m));
        let out = sys::capture("nm", &["--defined-only", &obj.to_string_lossy()], dir)
            .unwrap_or_default();
        if !out
            .lines()
            .any(|l| l.split_whitespace().nth(2) == Some(sym))
        {
            let _ = std::fs::remove_dir_all(&tmp);
            continue;
        }
        let _ = sys::run("ar", &["d", &lib.to_string_lossy(), &m], dir);
        let gname = format!("{guard}.o");
        let staged = dir.join(&gname);
        std::fs::copy(&obj, &staged).map_err(|e| format!("cp: {e}"))?;
        sys::run_ok("ar", &["r", &lib.to_string_lossy(), &gname], dir)?;
        let _ = std::fs::remove_dir_all(&tmp);
    }
    Ok(())
}

/// has_sym <lib.a> <sym>：符号在归档中有定义（不只被引用）。
/// nm 的已定义符号行总是 <hex 地址> <类型字母> <名字>；未定义是 `U name`。
/// Mach-O 给 C 名加下划线，容许可选一条；Windows nm 可能带 \r，先剥。
/// Defined (not merely referenced) symbol check. Defined rows always carry
/// a hex address + type letter; undefined print `U name`. Mach-O prepends
/// an underscore to C names — allow an optional one; strip \r first since
/// Windows nm may emit CRLF.
pub fn has_sym(lib: &Path, sym: &str, dir: &Path) -> bool {
    if !lib.is_file() {
        return false;
    }
    let matches = |out: &str| -> bool {
        out.lines().any(|l| {
            let mut it = l.split_whitespace();
            let is_def = it
                .next()
                .map(|a| a.chars().all(|c| c.is_ascii_hexdigit()) && !a.is_empty())
                .unwrap_or(false)
                && it
                    .next()
                    .map(|t| t.len() == 1 && t.chars().next().unwrap().is_ascii_alphabetic())
                    .unwrap_or(false);
            is_def
                && it
                    .next()
                    .map(|n| n == sym || n == format!("_{sym}"))
                    .unwrap_or(false)
        })
    };
    let out = sys::capture("nm", &[&lib.to_string_lossy()], dir).unwrap_or_default();
    if matches(&out) {
        return true;
    }
    // 选用的 nm 可能不适合这个归档（外来工具链、需 LTO 插件、或不存在），
    // 宣判缺席前先问编译器要它自己的 nm
    // the chosen nm may be wrong for this archive (foreign toolchain, LTO
    // plugin needed, or absent) — ask the compiler for its nm before
    // declaring the symbol absent
    let e = sys::env();
    let nm2 = sys::capture(&e.cc, &["-print-prog-name=nm"], dir).unwrap_or_default();
    let nm2 = nm2.trim().replace('\r', "");
    if !nm2.is_empty() && Path::new(&nm2).is_file() {
        let out = sys::capture(&nm2, &[&lib.to_string_lossy()], dir).unwrap_or_default();
        return matches(&out);
    }
    false
}
