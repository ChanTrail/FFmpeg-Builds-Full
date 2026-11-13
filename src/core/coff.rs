//! GNU ar 归档与 COFF 对象符号表的纯 Rust 解析——Windows 上每 spawn
//! ~200ms，postfix 的逐成员 `ar x`+`nm` 循环是分钟级开销（实证
//! librav1e.a ~4000 成员的 member-wise 兜底烧掉 ~150s）。这里一次
//! 读入归档字节、原地解析成员头与符号表，全部扫描零子进程。
//! Pure-Rust GNU ar archive + COFF object symbol-table parsing — each
//! spawn costs ~200ms on Windows, so postfix's per-member `ar x`+`nm`
//! loops were minute-scale (empirical: ~150s on librav1e.a's ~4000
//! members). Read the archive once and walk member headers + symbol
//! tables in place — zero subprocesses for all scans.

use std::collections::BTreeSet;

/// ar 归档成员（名字已解析长名表/ BSD 形式）/ archive member (name
/// resolved through the long-name table / BSD form)
pub struct Member {
    /// 成员名（与 `ar t` 输出一致）/ member name (as `ar t` prints)
    pub name: String,
    /// 数据在归档内的偏移 / byte offset of the member payload
    pub off: usize,
    /// 数据长度 / payload length
    pub len: usize,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// 解析归档成员表。GNU 变体：`/`=符号索引、`//`=长名表、`/NNN`=表内
/// 偏移名、`name/`=短名；BSD `#1/N`=名嵌数据头。thin archive 与无法
/// 解析时返回空（调用方按"无成员"退化，不会误删）。
/// Parse the member table. GNU variant: `/`=symbol index, `//`=long-name
/// table, `/NNN`=name offset inside it, `name/`=short name; BSD `#1/N`
/// embeds the name atop the payload. Thin archives or unparseable input
/// yield an empty list (callers degrade to "no members", never mis-delete).
pub fn members(data: &[u8]) -> Vec<Member> {
    let mut out = Vec::new();
    if data.len() < 8 || &data[..8] != b"!<arch>\n" {
        return out;
    }
    // 先找到长名表 `//` 的数据区供 /NNN 名解析 / locate the `//`
    // long-name table first for `/NNN` resolution
    let mut strtab: (usize, usize) = (0, 0);
    let mut p = 8usize;
    while p + 60 <= data.len() {
        let size = atoi(&data[p + 48..p + 58]);
        let name_raw = std::str::from_utf8(&data[p..p + 16])
            .unwrap_or("")
            .trim_end();
        if name_raw == "//" {
            strtab = (p + 60, size);
        }
        p += 60 + size + (size & 1);
    }
    let str_get = |off: usize| -> Option<String> {
        let (so, sl) = strtab;
        if off >= sl || so + off >= data.len() {
            return None;
        }
        let b = &data[so + off..];
        let end = b
            .iter()
            .position(|&c| c == b'\n' || c == 0)
            .unwrap_or(b.len());
        let s = std::str::from_utf8(&b[..end])
            .ok()?
            .trim_end_matches('/')
            .to_string();
        Some(s)
    };
    let mut p = 8usize;
    while p + 60 <= data.len() {
        let hdr = &data[p..p + 60];
        if &hdr[58..60] != b"`\n" {
            break;
        }
        let size = atoi(&hdr[48..58]);
        let raw = std::str::from_utf8(&hdr[..16]).unwrap_or("").trim_end();
        let body = p + 60;
        // BSD `#1/N`：名字占数据头 N 字节 / BSD `#1/N`: the first N
        // payload bytes are the name
        if let Some(n) = raw
            .strip_prefix("#1/")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if n <= size && body + n <= data.len() {
                let name = String::from_utf8_lossy(&data[body..body + n]).to_string();
                out.push(Member {
                    name,
                    off: body + n,
                    len: size - n,
                });
            }
        } else if raw == "/" || raw == "//" || raw.starts_with("/SYM64") {
            // 索引/名表成员——不是对象，跳过 / index/name-table member —
            // not an object, skip
        } else if let Some(off) = raw.strip_prefix('/').and_then(|s| s.parse::<usize>().ok()) {
            if let Some(name) = str_get(off) {
                out.push(Member {
                    name,
                    off: body,
                    len: size,
                });
            }
        } else {
            out.push(Member {
                name: raw.trim_end_matches('/').to_string(),
                off: body,
                len: size,
            });
        }
        p += 60 + size + (size & 1);
    }
    out
}

fn atoi(b: &[u8]) -> usize {
    std::str::from_utf8(b)
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(0)
}

/// 成员符号分类 / per-member symbol classification
#[derive(Default)]
pub struct Symtab {
    /// external 定义（对应 nm 大写 TDBRCSWV——class EXTERNAL、节>0 或
    /// common/weak）/ external definitions (nm's uppercase TDBRCSWV —
    /// class EXTERNAL, section>0 or common/weak)
    pub ext_defs: BTreeSet<String>,
    /// 一切已定义符号名（含节符号/静态——对应宽松 nm 扫描）/ every
    /// defined name (incl. section/statics — the loose nm scan)
    pub all_defs: BTreeSet<String>,
    /// 未定义引用（sec=0,val=0 的 EXTERNAL/WEAK）/ undefined refs
    /// (EXTERNAL/WEAK with sec=0,val=0)
    pub undefs: BTreeSet<String>,
    /// import-object 成员声明的导入名（非 COFF symtab；None=普通对象）
    /// imported name declared by an import-object member (not a COFF
    /// symtab; None = regular object)
    pub imp_name: Option<String>,
}

/// 解析单个成员字节的符号表。COFF 对象读 18B 符号项；import object
/// （00 00 FF FF 签名）读偏移 20 的导入名——链接器会合成 `__imp_<name>`。
/// Parse one member's symbol table. COFF objects expose 18-byte symbol
/// entries; import objects (00 00 FF FF signature) carry the imported
/// name at offset 20 — the linker synthesizes `__imp_<name>` for it.
pub fn symtab(m: &[u8]) -> Symtab {
    let mut st = Symtab::default();
    if m.len() < 20 {
        return st;
    }
    // import object：Sig1=0、Sig2=0xFFFF；20B 头后接 NUL 结尾导入名
    // import object: Sig1=0, Sig2=0xFFFF; NUL-terminated import name
    // follows the 20-byte header
    if u16le(m, 0) == 0 && u16le(m, 2) == 0xFFFF {
        let b = &m[20..];
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        if let Ok(s) = std::str::from_utf8(&b[..end])
            && !s.is_empty()
        {
            st.imp_name = Some(s.to_string());
        }
        return st;
    }
    // COFF 头：Machine@0、NumberOfSections@2、TimeDateStamp@4、
    // PointerToSymbolTable@8、NumberOfSymbols@12、SizeOfOptionalHeader@16
    // COFF header: Machine@0, NumberOfSections@2, TimeDateStamp@4,
    // PointerToSymbolTable@8, NumberOfSymbols@12, SizeOfOptionalHeader@16
    let symptr = u32le(m, 8) as usize;
    let nsym = u32le(m, 12) as usize;
    if symptr == 0 || nsym == 0 {
        return st;
    }
    let stab = symptr + nsym * 18;
    let str_len = if stab + 4 <= m.len() {
        u32le(m, stab) as usize
    } else {
        0
    };
    let strbase = stab + 4;
    let str_end = strbase
        + str_len
            .saturating_sub(4)
            .min(m.len().saturating_sub(strbase));
    let name_of = |e: &[u8]| -> Option<String> {
        if u32le(e, 0) == 0 {
            let off = u32le(e, 4) as usize;
            if off < 4 || strbase + off - 4 >= str_end {
                return None;
            }
            let b = &m[strbase + off - 4..str_end.min(m.len())];
            let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            std::str::from_utf8(&b[..end]).ok().map(String::from)
        } else {
            let end = e[..8].iter().position(|&c| c == 0).unwrap_or(8);
            // COFF 对象符号名不走归档式 "/NNN"，仅按 8B 内联取
            // COFF object symbol names never use the archive-style
            // "/NNN" — read the 8-byte inline field only
            std::str::from_utf8(&e[..end]).ok().map(String::from)
        }
    };
    let mut i = 0usize;
    while i < nsym {
        let e = symptr + i * 18;
        if e + 18 > m.len() {
            break;
        }
        let ent = &m[e..e + 18];
        let value = u32le(ent, 8);
        let sec = i16::from_le_bytes([ent[12], ent[13]]);
        let class = ent[16];
        let naux = ent[17] as usize;
        if let Some(name) = name_of(ent) {
            // sec==0&&val==0 → 未定义；sec>0 或 common(0,val>0)/absolute
            // (-1) → 已定义。EXTERNAL(2)/WEAK_EXTERNAL(105) 进 ext_defs
            // （nm 大写型），其余 class 只进 all_defs
            // sec==0&&val==0 -> undefined; sec>0 or common(0,val>0)/
            // absolute(-1) -> defined. EXTERNAL(2)/WEAK_EXTERNAL(105)
            // feed ext_defs (nm's uppercase types); other classes only
            // feed all_defs
            if sec == 0 && value == 0 {
                if class == 2 || class == 105 {
                    st.undefs.insert(name);
                }
            } else {
                st.all_defs.insert(name.clone());
                if class == 2 || class == 105 {
                    st.ext_defs.insert(name);
                }
            }
        }
        i += 1 + naux;
    }
    st
}
