//! update-sources —— sources/*.yaml 的钉版更新器 / the sources/*.yaml
//! pin updater
//!
//! 职责：把构建期已删除的实时解析（默认分支 HEAD → GitHub/GitLab CI
//! 绿门 → 版本样式 tag 回退 → 化石排除）在更新期重跑，把结果以精确
//! commit 的形式回写进 yaml 的 pin: 键。构建驱动只读 pin/ref，本程序
//! 是唯一写 pin 的地方。
//! Job: re-run at update time the live resolution deleted from the build
//! path (default-branch HEAD -> GitHub/GitLab CI-green gate ->
//! version-looking tag fallback -> fossil exclusion) and write the
//! results back into the yaml pin: keys as exact commits. The build
//! driver only reads pin/ref; this program is the only writer of pin.
//!
//! 用法 / usage:
//!   update-sources [--root <dir>] [--only a,b,c] [--missing-only] [--dry-run]
//!   update-sources --lint [--root <dir>]
//!
//! 模式 / modes:
//!   (default)        解析并回写 pin，打印人读摘要 / resolve+write pins, print a human summary
//!   --dry-run        只解析与报告，不写文件 / resolve+report without writing
//!   --lint           离线 schema 校验：git 条目须有 pin 或 ref，pin 须
//!                    40-hex；违规 exit 1（无网络，CI 用）/ offline schema
//!                    validation: git entries need pin or ref, pins must be
//!                    40-hex; violations exit 1 (no network, for CI)
//!   --only a,b,c     只处理列名条目 / restrict to the named entries
//!   --missing-only   只处理尚无 pin 的条目（回填/新增场景）/ only
//!                    entries lacking a pin (backfill/new-entry scenario)
//!
//! 回写是行级手术：yaml 注释与人排版式逐位保留，pin 行整体由本程序
//! 拥有（替换或插入在条目块最后一行内容之后）/ the rewrite is line
//! surgery: yaml comments and human formatting are preserved bit-exact;
//! the pin line is wholly owned by this program (replaced in place or
//! inserted after the entry block's last content line).

use ffbuild::log;
use ffbuild::sources::{self, Spec};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use yaml_rust2::YamlLoader;

/// 解析并发上限：与预取一致的保守值——API 查询密集，再高容易撞限流
/// resolution concurrency cap: the same conservative value as prefetch —
/// API queries dominate and higher caps invite rate limiting
fn sema() -> &'static tokio::sync::Semaphore {
    static S: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    S.get_or_init(|| tokio::sync::Semaphore::new(6))
}

/// 命令行选项 / parsed command-line options
struct Opts {
    /// 仓库根目录 / repo root dir
    root: PathBuf,
    /// 只处理这些条目（空 = 全部）/ restrict to these entries (empty = all)
    only: Vec<String>,
    /// 只处理缺 pin 的条目 / only entries lacking a pin
    missing_only: bool,
    /// 不写文件 / do not write files
    dry_run: bool,
    /// 离线 schema 校验模式 / offline schema-validation mode
    lint: bool,
}

/// 解析命令行 / parse the command line
/// Params：
/// - args: &[String] —— argv[1..] / process args without argv[0]
fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        root: std::env::current_dir().map_err(|e| e.to_string())?,
        only: Vec::new(),
        missing_only: false,
        dry_run: false,
        lint: false,
    };
    let mut i = 0;
    while i < args.len() {
        // --opt=value 形态先拆 / split the --opt=value form first
        let (name, inline) = match args[i].split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (args[i].clone(), None),
        };
        let take = |i: &mut usize| -> Result<String, String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value / {name} 需要一个值"))
        };
        match name.as_str() {
            "--root" => o.root = PathBuf::from(take(&mut i)?),
            "--only" => {
                o.only = take(&mut i)?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--missing-only" => o.missing_only = true,
            "--dry-run" => o.dry_run = true,
            "--lint" => o.lint = true,
            other => {
                return Err(format!(
                    "unknown option / 未知选项: {other}\n\
                     usage / 用法: update-sources [--root <dir>] [--only a,b,c] \\\n         \
                     [--missing-only] [--dry-run] | --lint"
                ));
            }
        }
        i += 1;
    }
    Ok(o)
}

/// 一个待解析任务：文件内的一个 git 条目 / one resolution job: a git
/// entry inside one yaml file
struct Job {
    /// 所在 yaml 文件 / the yaml file it lives in
    file: PathBuf,
    /// 条目名 / entry name
    name: String,
    /// 仓源链（主仓+mirrors）/ repo chain (primary + mirrors)
    urls: Vec<String>,
    /// 既有 ref（branch/tag/sha 任意形态）/ the existing ref (any of
    /// branch/tag/sha forms)
    ref_: Option<String>,
    /// 现有 pin（摘要对比用）/ current pin (for the summary diff)
    old_pin: Option<String>,
}

/// 读取并解析一份 yaml 文件成文档 / read+yaml-parse one file
/// Params：
/// - p: &Path —— yaml 文件路径 / path of the yaml file
fn read_doc(p: &Path) -> Result<yaml_rust2::Yaml, String> {
    let text = ffbuild::fsx::read(p);
    let mut docs =
        YamlLoader::load_from_str(&text).map_err(|e| format!("{}: YAML: {e}", p.display()))?;
    match docs.len() {
        1 => Ok(docs.remove(0)),
        n => Err(format!("{}: expected 1 document, got {n}", p.display())),
    }
}

/// 收集 sources/*.yaml 里的全部解析任务：git 条目且 ref 非 sha 形
/// （sha 钉已冻结无可解析；archive/空规格跳过）/ collect every
/// resolution job across sources/*.yaml: git entries whose ref is not
/// sha-shaped (sha pins are frozen — nothing to resolve; archive/empty
/// specs skipped)
/// Params：
/// - root: &Path —— 仓库根 / repo root
fn collect_jobs(root: &Path) -> Result<Vec<Job>, String> {
    let dir = root.join("sources");
    let mut files: Vec<PathBuf> = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        // 只要顶层 *.yaml——子目录不属于 sources schema / top-level
        // *.yaml only — subdirectories are outside the sources schema
        if p.extension().is_some_and(|x| x == "yaml") && p.is_file() {
            files.push(p);
        }
    }
    files.sort();
    let mut jobs = Vec::new();
    for f in files {
        let fname = f
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let doc = read_doc(&f)?;
        for (name, spec) in sources::specs_from_yaml(&doc, &fname)? {
            if spec.git.is_none() {
                continue;
            }
            // lock 在场即冻结圣旨——机器永不触碰（lock 的唯一写入路径
            // 是停滞转换，而停滞条目转换后不再有解析必要）/ a lock is
            // the freeze writ — the machine never touches it (lock's
            // only write path is staleness conversion, after which no
            // resolution is ever needed again)
            if spec.lock.is_some() {
                continue;
            }
            let mut urls: Vec<String> = spec.git.iter().cloned().collect();
            urls.extend(spec.mirrors.iter().cloned());
            jobs.push(Job {
                file: f.clone(),
                name,
                urls,
                ref_: spec.ref_.clone(),
                old_pin: spec.pin.clone(),
            });
        }
    }
    Ok(jobs)
}

// ==================== 行级回写手术 / line-surgery rewrite ====================

/// 列 0 键行的键名提取：非空白开头、`key:` 形、键字符限定——注释行与
/// 缩进行返回 None / extract the key of a column-0 key line: non-blank
/// start, `key:` shape, restricted key charset — comment and indented
/// lines yield None
/// Params：
/// - line: &str —— 一行原文 / one raw line
fn key_of(line: &str) -> Option<String> {
    if line.starts_with([' ', '\t', '#']) || line.trim().is_empty() {
        return None;
    }
    let (key, _) = line.split_once(':')?;
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return None;
    }
    Some(key.to_string())
}

/// 缩进的指定键行？/ an indented key line of the given key?
/// Params：
/// - line: &str —— 一行原文 / one raw line
/// - key: &str —— 键名（pin|ref|lock）/ the key name
fn is_key_line(line: &str, key: &str) -> bool {
    (line.starts_with(' ') || line.starts_with('\t'))
        && line.trim_start().starts_with(&format!("{key}:"))
}

/// 单键操作 / single-key operation
#[derive(Debug, Clone, PartialEq)]
enum Op {
    /// 写入/替换键行（值, 注释依据）/ write or replace the key line (value, comment basis)
    Set(String, String),
    /// 拆除键行 / remove the key line
    Remove,
}

/// 单条目的三键回写操作集：lock/ref/pin 各司其职——lock 是冻结圣旨
/// （停滞转换时机器落它）；ref 是机器维护的偏好前进；pin 是 hash
/// oracle / one entry's three-key rewrite op set: lock/ref/pin each
/// with its own office — lock is the freeze writ (the machine writes
/// it on staleness conversion); ref is machine-maintained preference
/// advancement; pin is the hash oracle
#[derive(Debug, Clone, PartialEq, Default)]
struct Ops {
    /// pin 键操作 / pin key operation
    pin: Option<Op>,
    /// ref 键操作 / ref key operation
    ref_: Option<Op>,
    /// lock 键操作 / lock key operation
    lock: Option<Op>,
}

/// 行级手术：把 updates（名称 → 三键操作集）写进 yaml 文本，注释与
/// 版式逐位保留。键行已存在则整行替换（本程序拥有它写出的行），否则
/// 插入在条目块最后一行内容之后；Remove 整行删除。多键插入按
/// lock/ref/pin 序 / line surgery: write updates (name -> three-key op
/// set) into the yaml text, preserving comments and formatting
/// bit-exactly. An existing key line is replaced wholesale (this
/// program owns the lines it writes); otherwise inserted after the
/// entry block's last content line; Remove deletes the line outright.
/// Multi-key insertions stack in lock/ref/pin order
/// Params：
/// - text: &str —— 原 yaml 全文 / the original yaml text
/// - updates: &HashMap<String, Ops> —— 条目名 → 操作集
///   / entry name -> op set
fn rewrite_file_text(text: &str, updates: &HashMap<String, Ops>) -> Result<String, String> {
    if updates.is_empty() {
        return Ok(text.to_string());
    }
    // CRLF 防御：仓库文件是 LF（.gitattributes 强制），但万一工作树被
    // 转换过，按原 terminator 重组 / CRLF defense: the repo files are LF
    // (.gitattributes enforced), but should a working tree get converted
    // the text is rejoined with its original terminator
    let crlf = text.contains("\r\n");
    let term = if crlf { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + updates.len());
    let mut written: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end_matches('\r');
        match key_of(line) {
            Some(name) => {
                out.push(line.to_string());
                i += 1;
                // 条目块：连续的缩进行或空行——列 0 的注释/键行终结它 /
                // the entry block: consecutive indented or blank lines —
                // a column-0 comment or key line terminates it
                let mut block: Vec<String> = Vec::new();
                while i < lines.len() {
                    let l = lines[i].trim_end_matches('\r');
                    if l.starts_with(' ') || l.starts_with('\t') || l.trim().is_empty() {
                        block.push(l.to_string());
                        i += 1;
                    } else {
                        break;
                    }
                }
                if let Some(ops) = updates.get(&name) {
                    // 固定键序：lock → ref → pin / fixed key order:
                    // lock -> ref -> pin
                    for (key, op) in [("lock", &ops.lock), ("ref", &ops.ref_), ("pin", &ops.pin)]
                        .into_iter()
                        .filter_map(|(k, o)| o.as_ref().map(|o| (k, o)))
                    {
                        match op {
                            Op::Set(val, basis) => {
                                // YAML 类型陷阱防御：纯数字 sha（概率微小但
                                // 非零）与浮点形 tag（1.0）不加引号会被解析成
                                // 数字，verbatim 字符串语义丢失——条件引号化，
                                // verify_rewrite 重解析即闭环 / YAML type-trap
                                // defense: an all-digit sha (slim but nonzero
                                // odds) or a float-shaped tag (1.0) parses as
                                // a number unquoted, losing verbatim string
                                // semantics — conditional quoting, closed loop
                                // by verify_rewrite's re-parse
                                let bare =
                                    val.parse::<i64>().is_err() && val.parse::<f64>().is_err();
                                let shown = if bare {
                                    val.clone()
                                } else {
                                    format!("\"{val}\"")
                                };
                                let new_line = format!("  {key}: {shown}  # {basis}");
                                match block.iter().position(|l| is_key_line(l, key)) {
                                    Some(pos) => block[pos] = new_line,
                                    None => {
                                        // 插入点：块内最后一个非空行之后；
                                        // 空块（单行条目）紧跟键行 /
                                        // insertion point: after the last
                                        // non-blank line; an empty block
                                        // (single-line entry) lands right
                                        // after the key line
                                        let ins = block
                                            .iter()
                                            .rposition(|l| !l.trim().is_empty())
                                            .map(|p| p + 1)
                                            .unwrap_or(0);
                                        block.insert(ins, new_line);
                                    }
                                }
                            }
                            Op::Remove => {
                                if let Some(pos) = block.iter().position(|l| is_key_line(l, key)) {
                                    block.remove(pos);
                                }
                            }
                        }
                    }
                    written.push(name.clone());
                }
                out.extend(block);
            }
            None => {
                out.push(line.to_string());
                i += 1;
            }
        }
    }
    // 有更新目标却没找到键行——版式超出手术能力，响亮报错不硬写 /
    // an update target whose key line was never found — the formatting
    // exceeds the surgery's power; fail loudly rather than write garbage
    for name in updates.keys() {
        if !written.contains(name) {
            return Err(format!(
                "rewrite: key line for '{name}' not found — refusing to write / 找不到键行，拒绝回写"
            ));
        }
    }
    let mut joined = out.join(term);
    if text.ends_with('\n') {
        joined.push_str(term);
    }
    Ok(joined)
}

/// 回写自校验：重解析手术后文本，逐键确认值与意图一致——注释保持型
/// 手术的最大风险是静默错位，落盘前必须证伪 / rewrite self-check:
/// re-parse the post-surgery text and confirm every key matches its
/// intent — the top risk of comment-preserving surgery is silent
/// misalignment, which must be disproven before hitting disk
/// Params：
/// - text: &str —— 手术后文本 / the post-surgery text
/// - file: &str —— 文件名（错误信息用）/ file name (for error messages)
/// - updates: &HashMap<String, Ops> —— 条目名 → 操作集
///   / entry name -> op set
fn verify_rewrite(text: &str, file: &str, updates: &HashMap<String, Ops>) -> Result<(), String> {
    let mut docs = YamlLoader::load_from_str(text)
        .map_err(|e| format!("{file}: post-rewrite YAML broken: {e}"))?;
    if docs.len() != 1 {
        return Err(format!("{file}: post-rewrite doc count {}", docs.len()));
    }
    let specs = sources::specs_from_yaml(&docs.remove(0), file)?;
    let map: HashMap<String, Spec> = specs.into_iter().collect();
    for (name, ops) in updates {
        let spec = map
            .get(name)
            .ok_or_else(|| format!("{file}: post-rewrite entry '{name}' missing"))?;
        for (key, op, got) in [
            ("lock", &ops.lock, spec.lock.as_deref()),
            ("ref", &ops.ref_, spec.ref_.as_deref()),
            ("pin", &ops.pin, spec.pin.as_deref()),
        ] {
            match op {
                Some(Op::Set(val, _)) => match got {
                    Some(g) if g == val => {}
                    other => {
                        return Err(format!(
                            "{file}: post-rewrite {key} mismatch for '{name}': want {val}, got {other:?}"
                        ));
                    }
                },
                Some(Op::Remove) if got.is_some() => {
                    return Err(format!(
                        "{file}: post-rewrite {key} still present for '{name}' after removal / 拆除后仍在: {got:?}"
                    ));
                }
                Some(Op::Remove) => {}
                None => {}
            }
        }
    }
    Ok(())
}

// ==================== lint 模式 / lint mode ====================

/// 离线 schema 校验：git 条目须有 pin 或 ref；pin 须 40-hex；解析错误
/// 计违规。返回违规数 / offline schema validation: git entries need pin
/// or ref; pins must be 40-hex; parse errors count as violations.
/// Returns the violation count
/// Params：
/// - root: &Path —— 仓库根 / repo root
fn lint(root: &Path) -> usize {
    let dir = root.join("sources");
    let mut files: Vec<PathBuf> = Vec::new();
    match std::fs::read_dir(&dir) {
        Ok(rd) => {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "yaml") && p.is_file() {
                    files.push(p);
                }
            }
        }
        Err(e) => {
            eprintln!("lint: read_dir {}: {e}", dir.display());
            return 1;
        }
    }
    files.sort();
    let mut violations = 0usize;
    let mut git_entries = 0usize;
    for f in files {
        let fname = f
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let doc = match read_doc(&f) {
            Ok(d) => d,
            Err(e) => {
                println!("lint: {e}");
                violations += 1;
                continue;
            }
        };
        let specs = match sources::specs_from_yaml(&doc, &fname) {
            Ok(s) => s,
            Err(e) => {
                println!("lint: {e}");
                violations += 1;
                continue;
            }
        };
        for (name, spec) in specs {
            if spec.git.is_none() {
                continue;
            }
            git_entries += 1;
            // 三键不变量 / three-key invariants:
            // 1. lock 只指 sha / lock points only at a sha
            // 2. lock 独占——冻结终态不需要 ref/pin / lock is exclusive —
            //    a frozen end-state needs neither ref nor pin
            // 3. tag/branch 形 ref 必须有 pin 做 hash oracle（按名拉取
            //    被禁）/ a tag/branch ref must carry a pin as its hash
            //    oracle (fetching by name is forbidden)
            // 4. 无 lock 无 ref 时 pin 必须在场 / without lock or ref a
            //    pin must be present
            // 5. pin 须 40-hex / pins must be 40-hex
            if let Some(lock) = &spec.lock {
                if !sources::is_sha(lock) {
                    println!(
                        "lint: {fname}: '{name}' lock '{lock}' is not a 40-hex commit / lock 只指 sha"
                    );
                    violations += 1;
                }
                if spec.ref_.is_some() || spec.pin.is_some() {
                    println!(
                        "lint: {fname}: '{name}' has lock: alongside ref:/pin: — lock is exclusive / lock 与 ref/pin 互斥"
                    );
                    violations += 1;
                }
            } else if let Some(r) = &spec.ref_ {
                if !sources::is_sha(r) && spec.pin.is_none() {
                    println!(
                        "lint: {fname}: '{name}' ref '{r}' is a tag/branch but has no pin: oracle — run update-sources / tag/branch 形 ref 缺 pin oracle"
                    );
                    violations += 1;
                }
            } else if spec.pin.is_none() {
                println!(
                    "lint: {fname}: '{name}' has none of lock:/ref:/pin: — run update-sources"
                );
                violations += 1;
            }
            if let Some(pin) = &spec.pin
                && !sources::is_sha(pin)
            {
                println!("lint: {fname}: '{name}' pin '{pin}' is not a 40-hex commit");
                violations += 1;
            }
        }
    }
    println!(
        "lint: {git_entries} git entries, {violations} violation(s) / {git_entries} 个 git 条目，{violations} 处违规"
    );
    violations
}

// ==================== 主流程 / main flow ====================

fn main() {
    // 最小环境：git 助手的 cwd（sys::env().work）与 PATH 即可，不做
    // 平台工具链探测——本程序只有网络查询没有编译 / minimal env: the
    // git helpers' cwd (sys::env().work) plus PATH suffice — this
    // program queries the network and never compiles
    ffbuild::sys::smoke_env();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if opts.lint {
        std::process::exit(if lint(&opts.root) == 0 { 0 } else { 1 });
    }

    let jobs = match collect_jobs(&opts.root) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("update-sources: {e}");
            std::process::exit(2);
        }
    };
    // 过滤：--only 按名、--missing-only 按 pin 缺席 / filtering: --only
    // by name, --missing-only by pin absence
    let jobs: Vec<Job> = jobs
        .into_iter()
        .filter(|j| opts.only.is_empty() || opts.only.contains(&j.name))
        .filter(|j| !opts.missing_only || j.old_pin.is_none())
        .collect();
    if jobs.is_empty() {
        println!("update-sources: no matching entries / 没有匹配的条目");
        std::process::exit(0);
    }
    let total = jobs.len();
    println!(
        "update-sources: resolving {total} entries / 解析 {total} 个条目{}",
        if opts.dry_run { " (dry-run)" } else { "" }
    );

    // 并发解析：scope 线程 + 信号量上限（resolve_commit 内部 block_on
    // 要求非 tokio 线程上下文）/ parallel resolution: scoped threads +
    // semaphore cap (resolve_commit's inner block_on demands a
    // non-tokio thread context)
    let results: Mutex<HashMap<(PathBuf, String), Result<sources::Resolution, String>>> =
        Mutex::new(HashMap::new());
    let done = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for job in &jobs {
            let results = &results;
            let done = &done;
            s.spawn(move || {
                // panic 隔离：单条目炸（网络边缘的意外形态）不该拖死
                // 整批——scope 会传播 panic，catch_unwind 把它降级成
                // 该条目的失败 / panic isolation: one entry blowing up
                // (an unexpected shape at the network edge) must not
                // sink the whole batch — scope propagates panics, so
                // catch_unwind demotes them to per-entry failures
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _permit = sema().blocking_acquire().expect("semaphore closed");
                    let urls: Vec<&str> = job.urls.iter().map(String::as_str).collect();
                    sources::resolve_commit(&urls, job.ref_.as_deref())
                }))
                .unwrap_or_else(|_| Err("resolver panicked / 解析器 panic".to_string()));
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                match &r {
                    Ok(res) => println!(
                        "[{n}/{total}] {}: {} — {}",
                        job.name,
                        &res.sha[..10.min(res.sha.len())],
                        res.basis
                    ),
                    Err(e) => println!("[{n}/{total}] {}: FAILED — {e}", job.name),
                }
                results
                    .lock()
                    .unwrap()
                    .insert((job.file.clone(), job.name.clone()), r);
            });
        }
    });
    let results = results.into_inner().unwrap();

    // 按文件分组：更新集（要写 pin 的）与摘要行 / group by file: the
    // update sets (pins to write) and the summary lines
    // 停滞地平线：上游分支 HEAD 的 commit 日期超过此年龄即判定项目
    // 死亡——成熟库动辄数月无提交是常态，四年级的沉默才配叫停滞 /
    // staleness horizon: an upstream branch HEAD whose commit date is
    // older than this is declared dead — mature libraries routinely go
    // months without commits; only four-year-scale silence earns the
    // name stalled
    const STALE_SECS: i64 = 4 * 365 * 86400 + 86400;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut per_file: HashMap<PathBuf, HashMap<String, Ops>> = HashMap::new();
    let mut counts = [0usize; 5]; // new/advanced/unchanged/failed/locked / 新钉/前进/未变/失败/停滞转lock
    let mut failures: Vec<String> = Vec::new();
    for job in &jobs {
        let key = (job.file.clone(), job.name.clone());
        let Some(res) = results.get(&key) else {
            continue;
        };
        let (file, name) = (&job.file, &job.name);
        match res {
            Ok(r) => {
                let mut ops = Ops::default();
                let stalled = r.head_date.is_some_and(|d| now - d > STALE_SECS);
                // 当前生效 hash：ref 为 sha 时是 ref，否则是 pin——摘要
                // 分桶的比较基准 / the currently effective hash: the ref
                // itself when sha-shaped, else the pin — the comparison
                // baseline for summary bucketing
                let cur: Option<&str> = match &job.ref_ {
                    Some(rv) if sources::is_sha(rv) => Some(rv.as_str()),
                    _ => job.old_pin.as_deref(),
                };
                if stalled {
                    // 停滞转换：lock 落终态 hash，ref/pin 一并拆除——
                    // 死亡上游的最终状态即永恒真相，偏好与 oracle 再无
                    // 意义 / staleness conversion: lock the final-state
                    // hash, remove ref and pin together — a dead
                    // upstream's final state is eternal truth, leaving
                    // preference and oracle meaningless
                    counts[4] += 1;
                    let year = r.head_date.map(|d| 1970 + d / (365 * 86400)).unwrap_or(0);
                    ops.lock = Some(Op::Set(
                        r.sha.clone(),
                        format!(
                            "upstream stalled (HEAD ~{year}) — locked at final resolution / 上游停滞——锁定终态"
                        ),
                    ));
                    ops.ref_ = Some(Op::Remove);
                    ops.pin = Some(Op::Remove);
                } else {
                    // 摘要分桶：0=新钉 1=前进 2=未变 / summary buckets:
                    // 0=new 1=advanced 2=unchanged
                    // 前进只能向前：新解析与现值不同即探两边 commit
                    // 日期，新更老则持有不动（CI 红日回退老 tag 绝不
                    // 许倒退写入——lensfun 式化石油门实证）/ forward
                    // only: when the new resolution differs, probe both
                    // commit dates — an older newcomer is held, never
                    // written (a red-CI day must not regress entries to
                    // fossil tags — the lensfun evidence)
                    let bucket = match cur {
                        None => 0,
                        Some(old) if old == r.sha => 2,
                        Some(old) => {
                            let newd = ffbuild::net::commit_date(&job.urls[0], &r.sha);
                            let oldd = ffbuild::net::commit_date(&job.urls[0], old);
                            match (newd, oldd) {
                                (Some(n), Some(o)) if n < o => 2, // 倒退持有 / regression held
                                _ => 1,
                            }
                        }
                    };
                    counts[bucket] += 1;
                    if bucket == 2 {
                        continue;
                    }
                    if r.ref_is_branch {
                        // 分支形 ref：偏好是"跟分支走"本身，ref 行原样
                        // 保留，pin 物化 HEAD / branch-shaped ref: the
                        // preference is "follow the branch" itself — the
                        // ref line stands as-is, the pin materializes HEAD
                        ops.pin = Some(Op::Set(r.sha.clone(), r.basis.clone()));
                    } else if job.ref_.is_some() {
                        // tag/sha 形 ref：机器维护前进。tag 赢 → ref 写
                        // tag 名 + pin 做 hash oracle；HEAD 赢 → ref
                        // 直写 sha，pin 没必要（ref 为 hash 时 pin
                        // 冗余）/ tag/sha-shaped ref: the machine
                        // maintains advancement. A tag win -> ref writes
                        // the tag name + pin as hash oracle; a HEAD win
                        // -> ref writes the sha directly and the pin is
                        // redundant (a sha ref needs no oracle)
                        match &r.tag {
                            Some(t) => {
                                ops.ref_ = Some(Op::Set(t.clone(), r.basis.clone()));
                                ops.pin = Some(Op::Set(r.sha.clone(), r.basis.clone()));
                            }
                            None => {
                                ops.ref_ = Some(Op::Set(r.sha.clone(), r.basis.clone()));
                                ops.pin = Some(Op::Remove);
                            }
                        }
                    } else {
                        // 无 ref：机器全自动，pin 是唯一落点 / no ref:
                        // full machine automation, the pin is the only
                        // landing spot
                        ops.pin = Some(Op::Set(r.sha.clone(), r.basis.clone()));
                    }
                }
                per_file
                    .entry(file.clone())
                    .or_default()
                    .insert(name.clone(), ops);
            }
            Err(e) => {
                counts[3] += 1;
                failures.push(format!("{}: {name}: {e}", file.display()));
            }
        }
    }

    // 回写（或 dry-run 报告）/ write back (or report in dry-run)
    let mut files_changed = 0usize;
    let mut files_written = 0usize;
    let mut sorted: Vec<&PathBuf> = per_file.keys().collect();
    sorted.sort();
    for file in sorted {
        let updates = &per_file[file];
        let text = ffbuild::fsx::read(file);
        let fname = file
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        match rewrite_file_text(&text, updates) {
            Ok(new_text) => {
                if let Err(e) = verify_rewrite(&new_text, &fname, updates) {
                    eprintln!("update-sources: {e}");
                    continue;
                }
                if new_text == text {
                    continue;
                }
                files_changed += 1;
                if opts.dry_run {
                    println!("dry-run: {} would be rewritten / 将被回写", file.display());
                } else {
                    match ffbuild::fsx::write(file, &new_text) {
                        Ok(()) => {
                            files_written += 1;
                            println!("rewritten / 已回写: {}", file.display());
                        }
                        Err(e) => eprintln!("update-sources: write {}: {e}", file.display()),
                    }
                }
            }
            Err(e) => eprintln!("update-sources: {}: {e}", file.display()),
        }
    }

    // 摘要 / summary
    println!(
        "update-sources: {} new, {} advanced, {} unchanged, {} locked(stalled), {} failed; {} file(s) changed, {} written / 新钉 {}，前进 {}，未变 {}，停滞转lock {}，失败 {}；{} 个文件变化，{} 个落盘",
        counts[0],
        counts[1],
        counts[2],
        counts[4],
        counts[3],
        files_changed,
        files_written,
        counts[0],
        counts[1],
        counts[2],
        counts[4],
        counts[3],
        files_changed,
        files_written
    );
    if !failures.is_empty() {
        println!("failures / 失败清单:");
        for f in &failures {
            println!("  {f}");
        }
    }
    // 全灭才算硬失败——部分失败不该淹没已成功的更新（PR 照样有价
    // 值），但零成功说明网络/凭证级故障 / total wipeout is the only
    // hard failure — partial failure must not drown the successful
    // updates (the PR is still valuable), yet zero successes spells a
    // network/credential-level outage
    let attempted = results.len();
    // 浮动同样是成功解析——只有 ffmpeg 一员的运行不该被误判全灭 /
    // floating counts as a successful resolution too — a run covering
    // only ffmpeg must not read as a wipeout
    let succeeded = counts[0] + counts[1] + counts[2] + counts[4];
    if attempted > 0 && succeeded == 0 {
        log::die(
            "update-sources: every resolution failed — network or credentials? / 全部解析失败——网络或凭证故障？",
        );
    }
    std::process::exit(0);
}

// ==================== 单元测试 / unit tests ====================

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // 测试构造器的工学：逐字段赋值比全字面量初始化可读 / test-builder ergonomics: per-field assignment reads better than full literal init
mod tests {
    use super::*;

    /// 构造 pin 写入 updates 的便捷函数 / shorthand for building a
    /// pin-write updates map
    fn upd(pairs: &[(&str, &str, &str)]) -> HashMap<String, Ops> {
        pairs
            .iter()
            .map(|(n, s, b)| {
                let mut ops = Ops::default();
                ops.pin = Some(Op::Set(s.to_string(), b.to_string()));
                (n.to_string(), ops)
            })
            .collect()
    }

    #[test]
    fn insert_after_last_content_line() {
        let text = "x264:\n  git: https://example.com/x264.git   # trailing comment\nx265:\n  git: https://example.com/x265.git\n";
        let out = rewrite_file_text(
            text,
            &upd(&[("x264", "a".repeat(40).as_str(), "master HEAD CI-green")]),
        )
        .unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "x264:");
        assert_eq!(
            lines[1],
            "  git: https://example.com/x264.git   # trailing comment"
        );
        assert_eq!(
            lines[2],
            format!("  pin: {}  # master HEAD CI-green", "a".repeat(40))
        );
        assert_eq!(lines[3], "x265:");
    }

    #[test]
    fn replace_existing_pin_preserving_neighbors() {
        let sha_old = "0".repeat(40);
        let sha_new = "1a".repeat(20);
        let text = format!(
            "# header comment\nxeve:\n  git: https://example.com/xeve.git\n  ref: v0.7.1   # human note\n  pin: {sha_old}  # old basis\nvvenc:\n"
        );
        let out = rewrite_file_text(&text, &upd(&[("xeve", &sha_new, "tag v0.7.1")])).unwrap();
        assert!(out.contains("# header comment"));
        assert!(out.contains(&format!("  pin: {sha_new}  # tag v0.7.1")));
        assert!(!out.contains(&sha_old));
        assert!(out.ends_with("vvenc:\n"));
    }

    #[test]
    fn comments_and_blank_lines_preserved_bitexact() {
        let sha = "f".repeat(40);
        let text = "# section\n\nfoo:\n  git: https://example.com/foo.git\n\n# inter-section comment\nbar:\n  git: https://example.com/bar.git\n";
        let out = rewrite_file_text(text, &upd(&[("foo", &sha, "b")])).unwrap();
        assert!(out.contains("\n\n# inter-section comment\nbar:"));
        // pin 落在块内最后一个内容行之后、空行与注释之前 / the pin lands
        // after the block's last content line, before blanks and comments
        let pos_pin = out.find("  pin:").unwrap();
        let pos_blank = out.find("\n\n# inter-section").unwrap();
        assert!(pos_pin < pos_blank);
    }

    #[test]
    fn mirrors_block_insertion() {
        let sha = "e".repeat(40);
        let text = "ffmpeg:\n  git: git://git.ffmpeg.org/ffmpeg.git            # note\n  mirrors:                                        # mirrors\n    - https://github.com/FFmpeg/FFmpeg.git\n  ref: master              # override note\n";
        let out = rewrite_file_text(
            text,
            &upd(&[("ffmpeg", &sha, "branch master HEAD CI-green")]),
        )
        .unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[4], "  ref: master              # override note");
        assert_eq!(
            lines[5],
            format!("  pin: {sha}  # branch master HEAD CI-green")
        );
    }

    #[test]
    fn missing_key_line_is_an_error() {
        let text = "foo:\n  git: https://example.com/foo.git\n";
        let r = rewrite_file_text(text, &upd(&[("nope", &"a".repeat(40), "b")]));
        assert!(r.is_err());
    }

    #[test]
    fn key_of_variants() {
        assert_eq!(key_of("x264:"), Some("x264".to_string()));
        assert_eq!(
            key_of("fdk-aac:                   # libfdk-aac"),
            Some("fdk-aac".to_string())
        );
        assert_eq!(key_of("  git: x"), None);
        assert_eq!(key_of("# comment"), None);
        assert_eq!(key_of(""), None);
        assert_eq!(key_of("- item"), None);
    }

    #[test]
    fn verify_roundtrip() {
        let sha = "c".repeat(40);
        let text = "foo:\n  git: https://example.com/foo.git\n";
        let updates = upd(&[("foo", &sha, "b")]);
        let out = rewrite_file_text(text, &updates).unwrap();
        verify_rewrite(&out, "test.yaml", &updates).unwrap();
    }

    #[test]
    fn removal_deletes_pin_line() {
        // 存量 pin 拆除：pin 行整行剔除，其余逐位不动 / stale-pin
        // removal: the pin line comes out wholesale, everything else
        // bit-identical
        let text = "ffmpeg:\n  git: https://example.com/ffmpeg.git\n  ref: master\n  pin: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  # branch master HEAD\nnext:\n  git: https://example.com/next.git\n";
        let mut updates = HashMap::new();
        let mut ops = Ops::default();
        ops.pin = Some(Op::Remove);
        updates.insert("ffmpeg".to_string(), ops);
        let out = rewrite_file_text(text, &updates).unwrap();
        assert_eq!(
            out,
            "ffmpeg:\n  git: https://example.com/ffmpeg.git\n  ref: master\nnext:\n  git: https://example.com/next.git\n"
        );
        // 拆除通过自校验 / the removal passes self-verification
        verify_rewrite(&out, "test.yaml", &updates).unwrap();
    }

    #[test]
    fn removal_without_pin_is_identity() {
        // 无 pin 可拆时操作是恒等——文件不应落盘（文本相同）/ with no
        // pin to remove the operation is the identity — the file must
        // not hit disk (identical text)
        let text = "ffmpeg:\n  git: https://example.com/ffmpeg.git\n  ref: master\n";
        let mut updates = HashMap::new();
        let mut ops = Ops::default();
        ops.pin = Some(Op::Remove);
        updates.insert("ffmpeg".to_string(), ops);
        let out = rewrite_file_text(text, &updates).unwrap();
        assert_eq!(out, text);
    }

    #[test]
    fn removal_key_missing_still_errors() {
        // 拆除目标键行不存在同样响亮报错 / a removal target whose key
        // line is missing fails loudly all the same
        let text = "foo:\n  git: https://example.com/foo.git\n";
        let mut updates = HashMap::new();
        let mut ops = Ops::default();
        ops.pin = Some(Op::Remove);
        updates.insert("nope".to_string(), ops);
        assert!(rewrite_file_text(text, &updates).is_err());
    }

    #[test]
    fn lock_conversion_writes_lock_removes_ref_and_pin() {
        // 停滞转换形态：lock 写入、ref/pin 拆除一次成型 / the
        // staleness-conversion shape: lock written, ref and pin removed
        // in one pass
        let sha = "d".repeat(40);
        let text = "deadlib:\n  git: https://example.com/dead.git\n  ref: v1.0\n  pin: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  # old\n";
        let mut updates = HashMap::new();
        let mut ops = Ops::default();
        ops.lock = Some(Op::Set(sha.clone(), "upstream stalled".to_string()));
        ops.ref_ = Some(Op::Remove);
        ops.pin = Some(Op::Remove);
        updates.insert("deadlib".to_string(), ops);
        let out = rewrite_file_text(text, &updates).unwrap();
        assert!(out.contains(&format!("  lock: {sha}  # upstream stalled")));
        assert!(!out.contains("  ref:"));
        assert!(!out.contains("  pin:"));
        verify_rewrite(&out, "test.yaml", &updates).unwrap();
    }

    #[test]
    fn ref_advancement_tag_win_writes_ref_and_pin() {
        // tag 赢形态：ref 改写成 tag 名、pin 落 oracle hash / the
        // tag-win shape: ref rewritten to the tag name, pin carries the
        // oracle hash
        let sha = "b".repeat(40);
        let text = "lib:\n  git: https://example.com/lib.git\n  ref: v1.0\n  pin: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  # old\n";
        let mut updates = HashMap::new();
        let mut ops = Ops::default();
        ops.ref_ = Some(Op::Set(
            "v2.0".to_string(),
            "release track — tag v2.0".to_string(),
        ));
        ops.pin = Some(Op::Set(sha.clone(), "release track — tag v2.0".to_string()));
        updates.insert("lib".to_string(), ops);
        let out = rewrite_file_text(text, &updates).unwrap();
        assert!(out.contains("  ref: v2.0  # release track — tag v2.0"));
        assert!(out.contains(&format!("  pin: {sha}  # release track — tag v2.0")));
        verify_rewrite(&out, "test.yaml", &updates).unwrap();
    }

    #[test]
    fn ref_advancement_head_win_writes_sha_removes_pin() {
        // HEAD 赢形态：ref 直写 sha、pin 拆除（sha ref 无需 oracle）/
        // the HEAD-win shape: ref writes the sha directly, pin removed
        // (a sha ref needs no oracle)
        let sha = "9a".repeat(20);
        let text = "lib:\n  git: https://example.com/lib.git\n  ref: v1.0\n  pin: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  # old\n";
        let mut updates = HashMap::new();
        let mut ops = Ops::default();
        ops.ref_ = Some(Op::Set(
            sha.clone(),
            "branch master HEAD CI-green".to_string(),
        ));
        ops.pin = Some(Op::Remove);
        updates.insert("lib".to_string(), ops);
        let out = rewrite_file_text(text, &updates).unwrap();
        assert!(out.contains(&format!("  ref: {sha}  # branch master HEAD CI-green")));
        assert!(!out.contains("  pin:"));
        verify_rewrite(&out, "test.yaml", &updates).unwrap();
    }
}
