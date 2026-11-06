//! 库源码清单与解析：sources/*.yaml 声明每个库从哪儿取（git 仓库实时
//! 获取 + CI 验证回退 tag；或固定归档），本模块把规格解析成实际检出，
//! 并提供后台预取——下载与编译并发执行。
//! Library source map and resolution: sources/*.yaml declares where each
//! library comes from (git repos fetched in real time with CI-verified
//! tag fallback; or fixed archives); this module resolves a spec into an
//! actual checkout and offers background prefetch — downloads run
//! concurrently with compilation.
//!
//! 布局 / layout:
//!   sources/common.yaml         {lib: {git|archive: ..., ref:, sha256:, submodules:}}
//!   sources/misc.yaml           {<os>-<arch>: {lib: spec}}   —— 按平台分节
//!   sources/<os>-<arch>.yaml    {lib: spec}                  —— 整值覆盖
//!
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
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use yaml_rust2::{Yaml, YamlLoader};

/// 单个库的源码规格 / one library's source spec
#[derive(Debug, Clone)]
pub struct Spec {
    /// git 仓库地址 / git repo URL
    pub git: Option<String>,
    /// 固定归档地址 / fixed archive URL
    pub archive: Option<String>,
    /// 可选 ref（分支/tag/sha）/ optional ref (branch|tag|sha)
    pub ref_: Option<String>,
    /// archive 的 sha256 / sha256 for archive
    pub sha256: Option<String>,
    /// git 检出后递归初始化子模块 / init submodules after checkout
    pub submodules: bool,
}

/// 加载并合并 sources 清单（common → misc[平台] → 平台文件；同键整值替换）
/// Load+merge the sources map (common → misc[platform] → platform file;
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
        let miscf = dir.join("misc.yaml");
        if miscf.is_file() {
            match read(&miscf) {
                Ok(m) => {
                    if !m[plat.as_str()].is_badvalue()
                        && let Err(e) = absorb(&m[plat.as_str()], "misc.yaml")
                    {
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

/// 把规格物化成 <wd>/src 检出 / materialise a spec into <wd>/src
fn fetch_into(wd: &Path, spec: &Spec) -> Result<PathBuf, String> {
    if let Some(url) = &spec.archive {
        return net::fetch_tar(wd, url, spec.sha256.as_deref());
    }
    let url = spec.git.as_ref().unwrap();
    let src = match resolve_git(url, spec.ref_.as_deref())? {
        Checkout::Commit(sha) => net::fetch_git_commit(wd, url, &sha)?,
        Checkout::Tag(tag) => net::fetch_git(wd, url, &tag, None)?,
    };
    if spec.submodules {
        sys::run_ok(
            "git",
            &[
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--depth=1",
                "--filter=blob:none",
            ],
            &src,
        )?;
    }
    Ok(src)
}

// ==================== 预取缓存 / prefetch cache ====================

/// 预取缓存目录 <work>/src-cache / prefetch cache dir
fn cache_dir() -> PathBuf {
    sys::env().work.join("src-cache")
}

/// 单个预取任务的句柄类型 / the join-handle type of one prefetch task
type Prefetch = tokio::task::JoinHandle<Result<(), String>>;

/// 预取任务登记表：lib → JoinHandle / in-flight prefetch tasks
fn tasks() -> &'static Mutex<HashMap<String, Prefetch>> {
    static T: OnceLock<Mutex<HashMap<String, Prefetch>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 并发上限：下载+CI 查询同时在跑不超过 4 个 / bound in-flight prefetches
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
            net::rt().spawn(async move {
                let _permit = sema().acquire().await.expect("semaphore closed");
                let r = tokio::task::spawn_blocking(move || -> Result<(), String> {
                    let spec = spec(&name2)?;
                    fsx::rm_rf(&cd);
                    fsx::install_d(&cd)?;
                    fetch_into(&cd, spec)?;
                    fsx::write(&cd.join("done"), "")?;
                    Ok(())
                })
                .await
                .map_err(|e| format!("prefetch task: {e}"))?;
                if let Err(e) = &r {
                    log::warn(&format!("[prefetch] {e}"));
                }
                r
            }),
        );
    }
}

/// 等待并取走预取结果到 wd（cache/src → wd/src 整树复制；预取未跑/失败
/// 则现场解析）/ await a prefetched source and copy it into wd (missing or
/// failed prefetch falls back to resolving on the spot)
pub fn materialize(wd: &Path, name: &str) -> Result<PathBuf, String> {
    let handle = tasks().lock().unwrap().remove(name);
    let cd = cache_dir().join(name);
    if let Some(h) = handle {
        match net::rt().block_on(h) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => log::warn(&format!(
                "[sources] prefetch {name} failed: {e} — resolving inline"
            )),
            Err(e) => log::warn(&format!("[sources] prefetch {name} join failed: {e}")),
        }
    }
    let dst = wd.join("src");
    if cd.join("done").is_file() && cd.join("src").is_dir() {
        fsx::rm_rf(&dst);
        copy_dir(&cd.join("src"), &dst)?;
        return Ok(dst);
    }
    // 未预取/预取失败：现场解析 / no (or failed) prefetch: resolve inline
    fetch_into(wd, spec(name)?)
}

/// 递归复制目录 / recursive directory copy
fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    for f in fsx::walk(src) {
        let rel = f.strip_prefix(src).map_err(|e| e.to_string())?;
        let to = dst.join(rel);
        if f.is_dir() {
            fsx::install_d(&to)?;
        } else {
            fsx::install(&f, &to)?;
        }
    }
    Ok(())
}

/// `ffbuild sources` 调试子命令：打印解析结果
/// Debug subcommand: print resolved sources
pub fn dump() -> Result<(), String> {
    for (name, s) in load() {
        let kind = if s.git.is_some() { "git" } else { "archive" };
        let url = s.git.as_deref().or(s.archive.as_deref()).unwrap_or("");
        println!(
            "{name}: {kind} {url}{}",
            s.ref_
                .as_deref()
                .map(|r| format!(" ref={r}"))
                .unwrap_or_default()
        );
    }
    Ok(())
}

// ==================== 测试 / tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// libsndfile 解析实测：默认分支 HEAD → GitHub CI 验证 → 失败回退
    /// 最新 tag。依赖外网+GitHub API，默认忽略。
    /// Live libsndfile resolution: default-branch HEAD -> GitHub CI gate ->
    /// fallback to the newest tag. Hits the network + GitHub API; ignored
    /// by default.
    #[test]
    #[ignore = "hits github.com + the Actions API; run explicitly"]
    fn libsndfile_resolve() {
        crate::sys::smoke_env();
        let (branch, sha) =
            crate::net::default_branch("https://github.com/libsndfile/libsndfile.git").unwrap();
        println!("default branch: {branch} @ {sha}");
        let ci = crate::net::ci_green_sync("https://github.com/libsndfile/libsndfile.git", &sha);
        println!("CI green: {ci:?}");
        let co = resolve_git("https://github.com/libsndfile/libsndfile.git", None).unwrap();
        match &co {
            Checkout::Commit(s) => println!("resolved: commit {s}"),
            Checkout::Tag(t) => println!("resolved: tag {t}"),
        }
    }
}
