//! 包清单加载：packages/<os>.yaml → 各操作系统的待装包列表
//! Package manifests: packages/<os>.yaml -> the package list per OS
//! 布局 / layout:
//!   packages/linux.yaml    [build-essential, gcc-{GCCV}, ...]   —— apt 包名数组
//!   packages/macos.yaml    [ccache, autoconf, ...]              —— brew 包名数组
//!   packages/windows.yaml  {libx264: {archs: all, flags: --enable-libx264}}  —— pacman 表
//! 规则 / rules:
//! * 数组元素必须是字符串——每行一个包名，原样进包管理器命令行
//!   array entries must be strings — one package name per line, passed
//!   verbatim to the package manager
//! * 映射键 = 去掉 mingw-w64-<env>- 前缀的包名；值可以是 null 或
//!   {archs: "all"|"x86_64"|"arm64", flags: "..."}，两字段都可缺省
//!   map key = package name without the mingw-w64-<env>- prefix; the
//!   value is either null or {archs: "all"|"x86_64"|"arm64",
//!   flags: "..."} — both fields optional
//! * flags 仅作记录——FFmpeg configure 旗标真源在 features/
//!   flags is documentation only — the configure-flag source of truth
//!   lives in features/

use crate::sys;
use yaml_rust2::{Yaml, YamlLoader};

/// 包表项 / package table entry
pub struct Pkg {
    /// 包名 / package name
    pub name: String,
    /// 适用架构："all" | "x86_64" | "arm64" / applicable arch
    pub archs: String,
    /// FFmpeg configure 旗标（仅记录；真源在 features/，运行时不读）
    /// FFmpeg configure flag (documentation only — features/ is the
    /// truth; unread at runtime)
    #[allow(dead_code)]
    pub flags: String,
}

/// 载入 packages/<file>.yaml。文件必须存在；空文档视为空表 / load
/// packages/<file>.yaml — the file must exist; an empty document reads
/// as an empty table
pub fn load(file: &str) -> Result<Vec<Pkg>, String> {
    let p = sys::env().repo_root.join("packages").join(file);
    if !p.is_file() {
        return Err(format!("{}: no such package manifest", p.display()));
    }
    let text = crate::fsx::read(&p);
    let docs = YamlLoader::load_from_str(&text)
        .map_err(|e| format!("{}: YAML parse error: {e}", p.display()))?;
    match docs.len() {
        1 => parse(&docs[0], file),
        n => Err(format!(
            "{}: expected exactly 1 document, got {n}",
            p.display()
        )),
    }
}

/// 解析单个 YAML 文档 <doc>：字符串数组 或 {name: null|{archs,flags}}
/// 映射（<file> 为错误定位用清单名）
/// parse one YAML document <doc>: a string array or a
/// {name: null|{archs,flags}} map (<file> is the manifest name used in
/// error messages)
fn parse(doc: &Yaml, file: &str) -> Result<Vec<Pkg>, String> {
    // 数组形：纯包名列表 / array form: bare package names
    if let Some(arr) = doc.as_vec() {
        let mut out = Vec::with_capacity(arr.len());
        for (i, v) in arr.iter().enumerate() {
            let name = v
                .as_str()
                .ok_or_else(|| format!("{file}: entry #{i} must be a string"))?;
            out.push(Pkg {
                name: name.trim().to_string(),
                archs: "all".into(),
                flags: String::new(),
            });
        }
        return Ok(out);
    }
    // 映射形：name → null | {archs?, flags?} / map form:
    // name -> null | {archs?, flags?}
    if let Some(h) = doc.as_hash() {
        let mut out = Vec::with_capacity(h.len());
        for (k, v) in h {
            let name = k
                .as_str()
                .ok_or_else(|| format!("{file}: package key must be a string"))?;
            let (mut archs, mut flags) = ("all".to_string(), String::new());
            match v {
                // `name:` 裸值 —— 全架构、无旗标 / bare `name:` — every
                // arch, no flags
                Yaml::Null | Yaml::BadValue => {}
                Yaml::Hash(f) => {
                    for (fk, fv) in f {
                        let key = fk.as_str().ok_or_else(|| {
                            format!("{file}: '{name}' field key must be a string")
                        })?;
                        match key {
                            "archs" => {
                                archs = fv
                                    .as_str()
                                    .ok_or_else(|| {
                                        format!("{file}: '{name}.archs' must be a string")
                                    })?
                                    .trim()
                                    .to_string();
                            }
                            "flags" => {
                                flags = fv
                                    .as_str()
                                    .ok_or_else(|| {
                                        format!("{file}: '{name}.flags' must be a string")
                                    })?
                                    .trim()
                                    .to_string();
                            }
                            other => {
                                return Err(format!("{file}: '{name}' unknown field '{other}'"));
                            }
                        }
                    }
                }
                _ => {
                    return Err(format!(
                        "{file}: '{name}' must be null or a {{archs, flags}} map, got {v:?}"
                    ));
                }
            }
            out.push(Pkg {
                name: name.to_string(),
                archs,
                flags,
            });
        }
        return Ok(out);
    }
    // 空文档（只有注释）→ 空表 / an all-comment document -> empty table
    if doc.is_badvalue() || matches!(doc, Yaml::Null) {
        return Ok(Vec::new());
    }
    Err(format!(
        "{file}: expected a string array or a name-keyed map, got {doc:?}"
    ))
}

/// 清单文件路径（windows_deps 的签名哈希锚点）/ manifest file path (the
/// signature-hash anchor for windows_deps)
pub fn file_path(file: &str) -> std::path::PathBuf {
    sys::env().repo_root.join("packages").join(file)
}
