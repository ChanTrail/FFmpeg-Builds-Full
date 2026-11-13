//! 库源码清单与解析：sources/*.yaml 声明每个库从哪儿取（git 仓库实时
//! 获取 + CI 验证回退 tag；或固定归档），本模块把规格解析成实际检出，
//! 并提供后台预取——下载与编译并发执行。
//! Library source map and resolution: sources/*.yaml declares where each
//! library comes from (git repos fetched in real time with CI-verified
//! tag fallback; or fixed archives); this module resolves a spec into an
//! actual checkout and offers background prefetch — downloads run
//! concurrently with compilation.
//! 布局 / layout:
//!   sources/common.yaml         {lib: {git|archive: ..., ref:, sha256:, submodules:}}
//!   sources/misc.yaml           {lib: spec}                  —— 不分平台的杂项
//!   sources/<os>-<arch>.yaml    {lib: spec}                  —— 整值覆盖
//! git 解析策略 / git resolution:
//!   1) ref 是 40-hex sha → 钉死检出
//!   2) ref 是 tag → 钉 tag；ref 是分支 → 该分支 HEAD 走第 3 步
//!   3) 无 ref → 默认分支 HEAD：GitHub/GitLab 查 CI——绿用该 commit；
//!      无 CI 或失败 → 最新 tag；连 tag 也没有 → HEAD（告警）

use crate::fsx;
use crate::log;
use crate::net;
use crate::sys;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use yaml_rust2::{Yaml, YamlLoader};

/// 单个库的源码规格 / one library's source spec
#[derive(Debug, Clone)]
pub struct Spec {
    /// git 仓库地址 / git repo URL
    pub git: Option<String>,
    /// git 仓源回退镜像（按序尝试，仅随 git: 生效）/ ordered git
    /// repo fallbacks (tried in order, only meaningful alongside git:)
    pub mirrors: Vec<String>,
    /// 固定归档地址 / fixed archive URL
    pub archive: Option<String>,
    /// 可选 ref（分支/tag/sha）/ optional ref (branch|tag|sha)
    pub ref_: Option<String>,
    /// archive 的 sha256 / sha256 for archive
    pub sha256: Option<String>,
    /// git 检出后递归初始化子模块 / init submodules after checkout
    pub submodules: bool,
}

/// 加载并合并 sources 清单（common → misc → 平台文件；同键整值替换）
/// Load+merge the sources map (common → misc → platform file;
/// a same-named key replaces the whole value)
fn load() -> &'static HashMap<String, Spec> {
    static MAP: OnceLock<HashMap<String, Spec>> = OnceLock::new();
    MAP.get_or_init(|| {
        let e = sys::env();
        let dir = e.repo_root.join("sources");
        let plat = e.plat();
        let mut map: HashMap<String, Spec> = HashMap::new();

        let mut absorb = |doc: &Yaml, file: &str| -> Result<(), String> {
            let h = doc
                .as_hash()
                .ok_or_else(|| format!("{file}: expected a lib: spec mapping"))?;
            for (k, v) in h {
                let name = k
                    .as_str()
                    .ok_or_else(|| format!("{file}: source key must be a string"))?;
                let spec = Spec {
                    git: v["git"].as_str().map(String::from),
                    mirrors: v["mirrors"]
                        .as_vec()
                        .map(|vs| {
                            vs.iter()
                                .filter_map(|u| u.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    archive: v["archive"].as_str().map(String::from),
                    ref_: v["ref"].as_str().map(String::from),
                    sha256: v["sha256"].as_str().map(String::from),
                    submodules: v["submodules"].as_bool().unwrap_or(false),
                };
                // 空规格（系统 SDK/闭源件，如 decklink）只登记不取源
                // an empty spec (system SDKs/proprietary pieces like
                // decklink) registers but fetches nothing
                if spec.git.is_none() && spec.archive.is_none() {
                    continue;
                }
                if spec.git.is_some() && spec.archive.is_some() {
                    return Err(format!(
                        "{file}: '{name}' needs exactly one of git: or archive:"
                    ));
                }
                if !spec.mirrors.is_empty() && spec.git.is_none() {
                    return Err(format!(
                        "{file}: '{name}' has mirrors: but no git: to mirror"
                    ));
                }
                map.insert(name.to_string(), spec);
            }
            Ok(())
        };

        let read = |p: &Path| -> Result<Yaml, String> {
            let text = fsx::read(p);
            let mut docs = YamlLoader::load_from_str(&text)
                .map_err(|e| format!("{}: YAML: {e}", p.display()))?;
            match docs.len() {
                1 => Ok(docs.remove(0)),
                n => Err(format!("{}: expected 1 document, got {n}", p.display())),
            }
        };

        // base.yaml 与 common.yaml 必备；misc/平台文件可选
        // base.yaml and common.yaml are required; misc/platform optional
        for (f, req) in [("base.yaml", true), ("common.yaml", true)] {
            match read(&dir.join(f)) {
                Ok(d) => {
                    if let Err(e) = absorb(&d, f) {
                        log::warn(&format!("sources/{f}: {e}"));
                    }
                }
                Err(e) if req => log::warn(&format!("sources/{f}: {e}")),
                _ => {}
            }
        }
        // misc.yaml 是扁平 {lib: spec}——源码只定位仓库，与平台无关 /
        // misc.yaml is a flat {lib: spec} — sources only locate the
        // repo and never differ by platform
        let miscf = dir.join("misc.yaml");
        if miscf.is_file() {
            match read(&miscf) {
                Ok(m) => {
                    if let Err(e) = absorb(&m, "misc.yaml") {
                        log::warn(&format!("sources/misc.yaml: {e}"));
                    }
                }
                Err(e) => log::warn(&format!("sources/misc.yaml: {e}")),
            }
        }
        let pf = dir.join(format!("{plat}.yaml"));
        if pf.is_file() {
            match read(&pf) {
                Ok(d) => {
                    if let Err(e) = absorb(&d, &format!("{plat}.yaml")) {
                        log::warn(&format!("sources/{plat}.yaml: {e}"));
                    }
                }
                Err(e) => log::warn(&format!("sources/{plat}.yaml: {e}")),
            }
        }
        map
    })
}

/// 取一个库的规格 / look up one library's spec
pub fn spec(name: &str) -> Result<&'static Spec, String> {
    load()
        .get(name)
        .ok_or_else(|| format!("sources: no entry for '{name}'"))
}

// ==================== git 解析 / git resolution ====================

/// git 解析结果：检出的具体形式 / resolved checkout form
enum Checkout {
    /// 精确 commit / an exact commit
    Commit(String),
    /// tag（浅克隆 --branch）/ a tag (shallow --branch clone)
    Tag(String),
}

/// 40-hex sha？/ is it a 40-hex sha?
fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// 解析 git 源成具体检出：ref 钉住 → 分支 HEAD → CI 验证 → tag 回退
/// Resolve a git source to a concrete checkout: a pinned ref → branch HEAD
/// → CI verification → tag fallback
fn resolve_git(url: &str, ref_: Option<&str>) -> Result<Checkout, String> {
    // 分支 HEAD → CI 门的共用决策 / shared decision for a branch HEAD
    let gate = |sha: &str, branch_desc: &str| -> Result<Checkout, String> {
        match net::ci_green_sync(url, sha) {
            Some(true) => {
                log::log(&format!(
                    "[sources] {url}: {branch_desc} HEAD {sha} CI green"
                ));
                Ok(Checkout::Commit(sha.to_string()))
            }
            v => {
                let why = match v {
                    Some(false) => "CI red or still running",
                    None => "no CI found",
                    _ => unreachable!(),
                };
                match net::latest_tag(url)? {
                    Some(tag) => {
                        log::log(&format!(
                            "[sources] {url}: {why} on {branch_desc} HEAD — falling back to tag {tag}"
                        ));
                        Ok(Checkout::Tag(tag))
                    }
                    None => {
                        log::warn(&format!(
                            "[sources] {url}: {why} and no tags — using {branch_desc} HEAD {sha}"
                        ));
                        Ok(Checkout::Commit(sha.to_string()))
                    }
                }
            }
        }
    };

    if let Some(r) = ref_ {
        if is_sha(r) {
            return Ok(Checkout::Commit(r.to_string()));
        }
        // tag 还是分支？/ tag or branch?
        let refs = net::ls_remote(url, &[r])?;
        let tag = refs.iter().any(|(_, rf)| rf == &format!("refs/tags/{r}"));
        let head = refs
            .iter()
            .find(|(_, rf)| rf == &format!("refs/heads/{r}"))
            .map(|(s, _)| s.clone());
        if tag && head.is_none() {
            log::log(&format!("[sources] {url}: pinned tag {r}"));
            return Ok(Checkout::Tag(r.to_string()));
        }
        if let Some(sha) = head {
            return gate(&sha, &format!("branch {r}"));
        }
        return Err(format!("sources: ref '{r}' not found on {url}"));
    }
    let (branch, sha) = net::default_branch(url)?;
    gate(&sha, &format!("default branch {branch}"))
}

/// git 源的归档兜底候选：主机 tarball 端点与检出内容等价但走
/// reqwest（可续传+自有重试），仅在 git 传输全灭后启用
/// archive fallback candidates for a git source: the hosts' tarball
/// endpoints carry identical content but ride reqwest (resumable +
/// own retries) — only engaged after the git transport is fully dead
fn archive_candidates(url: &str, rev: &str) -> Vec<String> {
    let url = url.trim_end_matches('/');
    // git:// 与 https:// 同主机——归档端点一律走 https（git.ffmpeg.org
    // 的 cgit snapshot 同样吃）/ git:// and https:// are the same
    // host — archive endpoints always ride https (git.ffmpeg.org's
    // cgit snapshot accepts them too)
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("git://"))
    else {
        return vec![];
    };
    let Some((host, path)) = rest.split_once('/') else {
        return vec![];
    };
    let noext = path.strip_suffix(".git").unwrap_or(path);
    let repo = noext.rsplit('/').next().unwrap_or(noext);
    let web = format!("https://{host}/{noext}");
    match host {
        // github 的 /archive/<rev> 接受 sha/分支/tag 三种原名；rev 是
        // tag 时 refs/tags/ 形式最精确——两候选都排上 / github's
        // /archive/<rev> accepts sha/branch/tag verbatim; the refs/tags
        // form is the precise spelling for tags — list both
        "github.com" => vec![
            format!("{web}/archive/{rev}.tar.gz"),
            format!("{web}/archive/refs/tags/{rev}.tar.gz"),
        ],
        "bitbucket.org" => vec![format!("{web}/get/{rev}.tar.gz")],
        h if h.ends_with("googlesource.com") => {
            vec![format!("{web}/+archive/{rev}.tar.gz")]
        }
        // gitlab.com 与自托管实例（freedesktop/videolan/gnome/xiph）同
        // 端点 / gitlab.com and the self-hosted instances
        // (freedesktop/videolan/gnome/xiph) share the endpoint
        h if h.starts_with("gitlab.") || h == "code.videolan.org" => {
            vec![format!("{web}/-/archive/{rev}/{repo}-{rev}.tar.gz")]
        }
        // 其余按 cgit 系赌一把 snapshot 端点（git.ffmpeg.org/libssh/
        // linuxtv/caoua，404 代价极低）/ everything else gets a cgit
        // snapshot guess (git.ffmpeg.org/libssh/linuxtv/caoua — a 404
        // costs very little)
        _ => vec![format!("{web}/snapshot/{rev}.tar.gz")],
    }
}

/// 把规格物化成 <wd>/src 检出 / materialise a spec into <wd>/src
fn fetch_into(wd: &Path, spec: &Spec) -> Result<PathBuf, String> {
    let src = if let Some(url) = &spec.archive {
        net::fetch_tar(wd, url, spec.sha256.as_deref())?
    } else {
        // 仓源链：git: 主仓 + mirrors: 顺序兜底——每条 URL 走完整
        // 解析+拉取；无子模块的源单 URL 全灭后转该仓主机的归档端点
        // （树内容与检出等价，.git 缺席由 materialize 死仓探活/配方
        // 自修复消化；reqwest 可续传+重试，远比整包重拉友好）/
        // repo chain: git: primary + mirrors: fallbacks — each URL
        // gets the full resolve+fetch; a submodule-free spec whose URL
        // dies outright falls back to that host's archive endpoints
        // (identical tree, and a missing .git is absorbed by
        // materialize's dead-repo probe / recipe self-repair; reqwest
        // resumes + retries — far kinder than re-pulling whole packs)
        let urls: Vec<&str> = spec
            .git
            .iter()
            .map(String::as_str)
            .chain(spec.mirrors.iter().map(String::as_str))
            .collect();
        let archive_fallback = |url: &str, rev: &str| -> Option<PathBuf> {
            if spec.submodules {
                return None;
            }
            // 先清场——失败的 clone 可能在 src/ 留下半截树，直接解包
            // 会混入残骸 / clear first — a failed clone can leave a
            // half tree in src/ and extracting over it mixes the debris
            // in
            let _ = std::fs::remove_dir_all(wd.join("src"));
            for au in archive_candidates(url, rev) {
                if let Ok(s) = net::fetch_tar(wd, &au, None) {
                    log::warn(&format!(
                        "[sources] {url}@{rev}: git transport failed — archive fallback {au} / git 传输失败转归档 {au}"
                    ));
                    return Some(s);
                }
            }
            None
        };
        // 单 URL 的完整尝试：解析 → 拉取，失败转本仓的归档端点 /
        // one URL's full attempt: resolve → fetch, falling to this
        // repo's own archive endpoints on transport death
        let try_url = |url: &str| -> Result<PathBuf, String> {
            match resolve_git(url, spec.ref_.as_deref()) {
                // 解析本身已死（ls-remote 停滞）——钉住的 ref 名仍可赌
                // 归档端点（github/gitlab 接受分支/tag 原名）/ resolution
                // itself died (ls-remote stall) — a pinned ref name can
                // still try the archive endpoints (github/gitlab accept
                // raw branch/tag names)
                Err(e) => spec
                    .ref_
                    .as_deref()
                    .filter(|&r| !is_sha(r))
                    .and_then(|r| archive_fallback(url, r))
                    .ok_or(e),
                Ok(co) => {
                    let (rev, git_res) = match &co {
                        Checkout::Commit(s) => (s.clone(), net::fetch_git_commit(wd, url, s)),
                        Checkout::Tag(t) => (t.clone(), net::fetch_git(wd, url, t, None)),
                    };
                    git_res.or_else(|e| archive_fallback(url, &rev).ok_or(e))
                }
            }
        };
        let mut last_err = String::new();
        let mut got = None;
        for (i, url) in urls.iter().enumerate() {
            if i > 0 {
                log::warn(&format!("[sources] trying mirror {url} / 换镜像仓 {url}"));
            }
            match try_url(url) {
                Ok(s) => {
                    got = Some(s);
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        let src = got.ok_or(last_err)?;
        if spec.submodules {
            // submodule 拉取同属网络传输——收进 git_run 的停滞探测+退
            // 避重试（-c 经 GIT_CONFIG_PARAMETERS 传到子 git 进程）/
            // submodule fetches are the same transport — under
            // git_run's stall detection + backoff retries (-c options
            // reach the child git processes via GIT_CONFIG_PARAMETERS)
            net::git_run(
                &[
                    "submodule",
                    "update",
                    "--init",
                    "--recursive",
                    "--depth=1",
                    "--filter=blob:none",
                ],
                &src,
                None,
            )?;
        }
        src
    };
    normalize_autotools_stamps(&src);
    Ok(src)
}

/// autotools 生成物清单：make 的维护者规则会按时间戳对这些文件调
/// aclocal-X.Y/automake-X.Y 重生成——版本号是"当年生成时"的，几乎从
/// 不与环境安装的一致
/// autotools generated files: make's maintainer rules regenerate these
/// via aclocal-X.Y/automake-X.Y keyed on timestamps — the version is the
/// one that generated them and almost never matches what's installed
/// 中间档：aclocal.m4 是双重身份——它由 configure.ac+m4/*.m4 生成
/// （须晚于全部输入），又是 Makefile.in/configure 的依赖
/// （am__configure_deps 含 $(ACLOCAL_M4)，须早于生成物）。放输入档
/// 会让它自己的重生成规则触发（walk 序 aclocal.m4 先于 configure.ac
/// 被 touch→更旧）；放生成档会让 Makefile.in 永远过期。只有独占一档
/// 卡在两者中间才同时满足。
/// Middle tier: aclocal.m4 is dual-natured — generated FROM
/// configure.ac+m4/*.m4 (must postdate all inputs) yet a prereq OF
/// Makefile.in/configure (am__configure_deps carries $(ACLOCAL_M4),
/// must predate generated files). At input tier its own regen rule
/// fires (walk order touches aclocal.m4 before configure.ac); at gen
/// tier Makefile.in goes permanently stale. A dedicated middle tier
/// satisfies both.
const AUTOTOOLS_MID: &[&str] = &["aclocal.m4"];

const AUTOTOOLS_GEN: &[&str] = &[
    "configure",
    "Makefile.in",
    "config.h.in",
    "config.hi.in",
    "config.hin",
    "stamp-h.in",
    "depcomp",
    "missing",
    "install-sh",
    "ltmain.sh",
    "config.guess",
    "config.sub",
    "compile",
    "ar-lib",
    "mkinstalldirs",
    "test-driver",
    "ylwrap",
];

/// 把 <p> 的 mtime 抬到当前时间 / bump <p>'s mtime to now
fn touch(p: &Path) {
    let _ = std::fs::File::options()
        .write(true)
        .open(p)
        .and_then(|f| f.set_modified(std::time::SystemTime::now()));
}

/// 时间戳归一化：tarball/git 源的 mtime 次序不可信（git checkout 全体等
/// 于检出时刻，copy_dir 又只看内容），一旦"输入"（configure.ac/*.am/
/// *.m4）比"生成物"（aclocal.m4/configure/Makefile.in）新，make 就走维
/// 护者规则调 aclocal-<生成版本>——生成版本几乎从不在环境里（镜像
/// automake 1.18 vs 包要 aclocal-1.16/1.10）。两遍 touch：先全体齐平，
/// 再把生成物抬新，输入永远不比生成物新 → 永不重生成。
/// Timestamp normalization: tarball/git mtime order is untrustworthy
/// (git checkouts stamp everything at checkout time, and copy_dir only
/// carries content) — once an input (configure.ac/*.am/*.m4) looks newer
/// than a generated file (aclocal.m4/configure/Makefile.in), make takes
/// the maintainer rule and invokes aclocal-<generating-version>, which
/// is virtually never installed (image has automake 1.18 while packages
/// demand aclocal-1.16/1.10). Three passes: equalize everything, lift
/// the middle tier (aclocal.m4), then lift the generated files — inputs
/// never look newer, so nothing regenerates.
fn normalize_autotools_stamps(src: &Path) {
    let files = fsx::walk(src);
    for f in &files {
        touch(f);
    }
    for f in &files {
        if f.file_name()
            .map(|n| AUTOTOOLS_MID.contains(&n.to_string_lossy().as_ref()))
            .unwrap_or(false)
        {
            touch(f);
        }
    }
    for f in &files {
        if f.file_name()
            .map(|n| AUTOTOOLS_GEN.contains(&n.to_string_lossy().as_ref()))
            .unwrap_or(false)
        {
            touch(f);
        }
    }
}

// ==================== 预取缓存 / prefetch cache ====================

/// 预取缓存目录 <work>/src-cache / prefetch cache dir
fn cache_dir() -> PathBuf {
    sys::env().work.join("src-cache")
}

/// 单个预取任务的句柄类型：std::thread——fetch_into 整条链路是同步的
/// （git/tar/HTTP 同步包装），跑在 tokio 线程上反而会让它内部的
/// block_on 撞 "Cannot start a runtime from within a runtime"；原生
/// 线程没有运行时上下文，block_on 合法。
/// The join-handle type of one prefetch task: a std::thread — fetch_into
/// is synchronous end to end (git/tar/sync HTTP wrappers); on a tokio
/// thread its inner block_on hits "Cannot start a runtime from within
/// a runtime", while a native thread carries no runtime context and may
/// legally block_on.
type Prefetch = std::thread::JoinHandle<Result<(), String>>;

/// 预取任务登记表：lib → JoinHandle / in-flight prefetch tasks
fn tasks() -> &'static Mutex<HashMap<String, Prefetch>> {
    static T: OnceLock<Mutex<HashMap<String, Prefetch>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 并发上限：下载+CI 查询同时在跑不超过 4 个 / bound in-flight prefetches
/// （std::thread 里用 blocking_acquire 取用——它专为非 async 上下文设计）
/// (threads take it via blocking_acquire — built for non-async contexts)
fn sema() -> &'static tokio::sync::Semaphore {
    static S: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    S.get_or_init(|| tokio::sync::Semaphore::new(4))
}

/// 后台预取一批库的源码：解析+检出进 src-cache（有 .done 标记即命中跳
/// 过）。任务句柄入库，ctx.src() 用名取用并等待。
/// Prefetch a batch of library sources in the background: resolve+checkout
/// into src-cache (a .done marker is a cache hit). Handles register by
/// name; ctx.src() takes and awaits them.
pub fn prefetch<I: IntoIterator<Item = String>>(names: I) {
    let cache = cache_dir();
    let _ = std::fs::create_dir_all(&cache);
    let mut t = tasks().lock().unwrap();
    for name in names {
        if t.contains_key(&name) {
            continue;
        }
        // 清单里没有的键（别的平台专属的源）直接跳过 / skip keys absent
        // from the merged map (platform-only sources on other platforms)
        if spec(&name).is_err() {
            continue;
        }
        let cd = cache.join(&name);
        if cd.join("done").is_file() {
            continue;
        }
        let name2 = name.clone();
        t.insert(
            name,
            std::thread::spawn(move || {
                let _permit = sema().blocking_acquire().expect("semaphore closed");
                let r = (|| -> Result<(), String> {
                    let spec = spec(&name2)?;
                    fsx::rm_rf(&cd);
                    fsx::install_d(&cd)?;
                    fetch_into(&cd, spec)?;
                    fsx::write(&cd.join("done"), "")?;
                    Ok(())
                })();
                if let Err(e) = &r {
                    log::warn(&format!("[prefetch] {e}"));
                }
                r
            }),
        );
    }
}

/// 只等 <name> 的预取任务落地（不复制）。recipe-run 子进程模式下父进程在
/// 派生配方前调用：预取句柄留在父进程，done 标记落盘，子进程的
/// materialize 走磁盘缓存命中而不是再次解析下载。
/// Await <name>'s prefetch without copying. Under the recipe-run
/// subprocess model the parent calls this before spawning the recipe:
/// the handle stays in the parent, the done marker lands on disk, and
/// the child's materialize hits the on-disk cache instead of resolving
/// again.
pub fn wait_prefetch(name: &str) {
    let handle = tasks().lock().unwrap().remove(name);
    if let Some(h) = handle {
        // 等待落一行日志——预取线程卡在 stalled 网络时这里是唯一的
        // 无声阻塞点，不留字句就是"日志莫名停住"的现场 / log the
        // wait — a prefetch thread jammed on a stalled transfer makes
        // this the one silent blocking point; without a line the log
        // just stops with no clue why
        log::log(&format!("[sources] awaiting prefetch {name}"));
        match h.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => log::warn(&format!(
                "[sources] prefetch {name} failed: {e} — child resolves inline"
            )),
            Err(p) => log::warn(&format!(
                "[sources] prefetch {name} panicked: {}",
                panic_msg(&p)
            )),
        }
    }
}

/// 等待并取走 <name> 的预取结果到 wd（cache/src → wd/src 整树复制；预取
/// 未跑/失败则现场解析）/ await <name>'s prefetched source and copy it
/// into wd (missing or failed prefetch falls back to resolving on the spot)
pub fn materialize(wd: &Path, name: &str) -> Result<PathBuf, String> {
    let handle = tasks().lock().unwrap().remove(name);
    let cd = cache_dir().join(name);
    if let Some(h) = handle {
        // 同 wait_prefetch：join 前留字句防无声卡顿 / same as
        // wait_prefetch: leave a trace before the join
        log::log(&format!("[sources] materialize {name}: awaiting prefetch"));
        match h.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => log::warn(&format!(
                "[sources] prefetch {name} failed: {e} — resolving inline"
            )),
            Err(p) => log::warn(&format!(
                "[sources] prefetch {name} panicked: {}",
                panic_msg(&p)
            )),
        }
    }
    let dst = wd.join("src");
    if cd.join("done").is_file() && cd.join("src").is_dir() {
        fsx::rm_rf(&dst);
        copy_dir(&cd.join("src"), &dst)?;
    } else {
        // 未预取/预取失败：现场解析 / no (or failed) prefetch: resolve inline
        fetch_into(wd, spec(name)?)?;
    }
    // 残壳 .git 比没有更糟：骗过上游构建脚本的 EXISTS 检查，然后
    // --git-dir 暴毙于 "not a git repository"。剥成缺席——上游改走
    // 非 git 回退路径，需要真仓库的配方（avisynth/libplacebo）自行
    // 修复 / a gutted .git is worse than none: it fools the EXISTS check
    // in upstream build scripts, then --git-dir dies on "not a git
    // repository". Reduce it to absent — upstream falls back to its
    // non-git path, and recipes needing a real repo (avisynth/libplacebo)
    // repair it themselves
    let git = dst.join(".git");
    // .git 在场就用探活定生死——目录形状（HEAD+objects）不够：copy_dir
    // 只收文件，全空的 refs/ 层蒸发后该形状仍在却已是死库。探针必须
    // 钉住 .git 本体（--git-dir）：裸 rev-parse 会沿父目录发现外层仓
    // 库（工作根就在 checkout 内），exit 0 把死壳当活库放行
    // / with a .git present, a liveness probe decides its fate — the
    // dir shape (HEAD+objects) isn't enough: copy_dir carries files
    // only, so an all-empty refs/ layer vanishes and the same shape is
    // already a dead repo. The probe must pin .git itself (--git-dir):
    // a bare rev-parse walks up into an enclosing repo (the work root
    // lives inside the checkout), exits 0, and keeps the corpse alive
    let probe = format!("--git-dir={}", git.display());
    if git.exists() && sys::run("git", &[&probe, "rev-parse", "--git-dir"], &dst) != Ok(0) {
        log::warn(&format!(
            "[sources] {name}: shipped .git fails rev-parse — removing it"
        ));
        fsx::rm_rf(&git);
    }
    // 补丁打在 wd 副本上——src-cache 保持原始检出干净 / patches land on
    // the wd copy — src-cache stays a pristine checkout
    crate::patches::apply(&dst, name)?;
    Ok(dst)
}

/// 把 join 收回的 panic payload 压成可读文本 / render a joined panic
/// payload into readable text
fn panic_msg(p: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// 递归复制目录：先建全目录树（含空目录）再拷文件——walk 只产文件，
/// 空目录不复制会留下语义差异：.git/refs/heads|tags 在 packed-ref 检出
/// 里就是空目录，整层蒸发让 .git 变死库（is_git_directory 要求 refs/
/// 存在），配方再去 init 只是补救 / recursive directory copy: build the
/// full dir tree (empties included) before files — walk yields files
/// only, and skipping empty dirs changes semantics: .git/refs/heads|tags
/// ARE empty under a packed-ref checkout; losing them turns .git into a
/// dead repo (is_git_directory requires refs/), with recipe re-init as
/// mere remediation
fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    // .git 子树允许条目蒸发：git 自己的 midx/bitmap 后台维护会留下
    // bitmap-ref-tips_* 之类临时文件，readdir/walk 快照到落地之间
    // 已被改名——跳过它（真死库反正由 materialize 的探活剥离）。
    // .git 之外保持硬炸：源文件凭空消失意味着 src-cache 正被并发
    // 改写，放过去只会得到一棵静默残缺的树 / tolerate evanescent
    // entries under .git: git's own background midx/bitmap maintenance
    // leaves temp files (bitmap-ref-tips_*) that get renamed between
    // the readdir/walk snapshot and landing — skip them (a dead repo
    // gets stripped by materialize's liveness probe anyway). Outside
    // .git keep hard-failing: a source file vanishing mid-copy means
    // the src-cache is being rewritten concurrently; passing that
    // yields a silently incomplete tree
    let git_skip = src.join(".git");
    let mut dirs = vec![src.to_path_buf()];
    while let Some(d) = dirs.pop() {
        if !d.exists() && d.starts_with(&git_skip) {
            continue;
        }
        for e in fs::read_dir(&d).map_err(|e| format!("read_dir {}: {e}", d.display()))? {
            let e = e.map_err(|e| format!("read_dir {}: {e}", d.display()))?;
            let p = e.path();
            let ft = match e.file_type() {
                Ok(ft) => ft,
                Err(_) if p.starts_with(&git_skip) => continue,
                Err(e) => return Err(format!("file_type {}: {e}", p.display())),
            };
            if ft.is_dir() {
                let rel = p.strip_prefix(src).map_err(|e| e.to_string())?;
                fsx::install_d(&dst.join(rel))?;
                dirs.push(p);
            }
        }
    }
    for f in fsx::walk(src) {
        if !f.exists() && f.starts_with(&git_skip) {
            continue;
        }
        let rel = f.strip_prefix(src).map_err(|e| e.to_string())?;
        let to = dst.join(rel);
        if f.is_dir() {
            fsx::install_d(&to)?;
        } else {
            fsx::install(&f, &to)?;
            // 保留 mtime：fs::copy 不保留时间戳，而 autotools 的
            // aclocal.m4/configure.ac 次序全靠它——次序一旦反转 make 会
            // 走维护者模式调 aclocal 重生成，在没装 automake 的环境直接炸
            // preserve mtime: fs::copy drops timestamps and autotools'
            // aclocal.m4/configure.ac ordering depends on them — reversed
            // order makes make invoke aclocal via maintainer rules, which
            // explodes where automake isn't installed
            if let Ok(md) = f.metadata()
                && let Ok(mt) = md.modified()
            {
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&to)
                    .and_then(|h| h.set_modified(mt));
            }
        }
    }
    copy_symlinks(src, dst)?;
    Ok(())
}

/// 复制 `src` 树里的全部符号链接——`walk` 只产真实文件/目录，链接会
/// 被静默丢弃（tarball 里常见：mysofa 的 default.sofa 数据文件、
/// alsa 的 include/alsa→. 头文件布局）。POSIX 原地重建链接；Windows
/// 物化成内容（fs::copy 跟随链接）——文件链接落成副本，目录链接
/// 落成真实目录副本，且内部再遇链接按文件策略逐个处理，不会环。
/// Copy every symlink under `src` — `walk` only yields real
/// files/dirs, links would be silently dropped (tarballs carry them:
/// mysofa's default.sofa data file, alsa's include/alsa->. header
/// layout). POSIX recreates links in place; Windows materializes
/// content (fs::copy dereferences) — a file link becomes a copy, a dir
/// link becomes a real dir copy, links nested inside are handled
/// per-file, no cycles.
fn copy_symlinks(src: &Path, dst: &Path) -> Result<(), String> {
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
            let e = e.map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
            let p = e.path();
            let ft = e
                .file_type()
                .map_err(|e| format!("file_type {}: {e}", p.display()))?;
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_symlink() {
                let rel = p.strip_prefix(src).map_err(|e| e.to_string())?;
                let to = dst.join(rel);
                if let Some(parent) = to.parent() {
                    fsx::install_d(parent)?;
                }
                if copy_one_symlink(&p, &to).is_err() {
                    log::warn(&format!(
                        "symlink {} dropped (dangling or unreadable)",
                        p.display()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// 物化单个符号链接 / Materialize one symlink
fn copy_one_symlink(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let target = fs::read_link(from).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&target, to).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        // 目录链接→真实目录副本，且只拷真实文件（walk 跳过链接）——
        // 自指链接如 include/alsa->. 不会无限递归；文件链接→内容
        // dir link -> real dir copy of real files only (walk skips
        // links) — self-referential links like include/alsa->. cannot
        // recurse forever; file link -> content
        if from.is_dir() {
            fsx::install_d(to)?;
            for f in fsx::walk(from) {
                if f.is_dir() {
                    continue;
                }
                let rel = f.strip_prefix(from).map_err(|e| e.to_string())?;
                fsx::install(&f, &to.join(rel))?;
            }
            Ok(())
        } else {
            fsx::install(from, to)
        }
    }
}

/// `ffbuild sources` 调试子命令：打印解析结果
/// Debug subcommand: print resolved sources
pub fn dump() -> Result<(), String> {
    for (name, s) in load() {
        let kind = if s.git.is_some() { "git" } else { "archive" };
        let url = s.git.as_deref().or(s.archive.as_deref()).unwrap_or("");
        println!(
            "{name}: {kind} {url}{}{}",
            s.ref_
                .as_deref()
                .map(|r| format!(" ref={r}"))
                .unwrap_or_default(),
            if s.mirrors.is_empty() {
                String::new()
            } else {
                format!(" mirrors=[{}]", s.mirrors.join(", "))
            }
        );
    }
    Ok(())
}
