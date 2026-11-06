//! 网络层：reqwest(rustls) 异步下载（流式落盘、断点续传、重试退避）、
//! GitHub/GitLab CI 查询、git ls-remote 助手，以及 tar/git 源码获取原语。
//! Network layer: async reqwest(rustls) downloads (streamed to disk, resume,
//! retry with backoff), GitHub/GitLab CI queries, git ls-remote helpers and
//! the tar/git source-fetch primitives.
//!
//! 同步/异步双形态：HTTP 是 async（供预取并发）；对外暴露 sync 包装
//! （rt().block_on），配方与构建代码保持同步。
//! Dual sync/async shape: HTTP is async (for prefetch concurrency); sync
//! wrappers (rt().block_on) keep recipes and build code synchronous.

use crate::fsx;
use crate::log;
use crate::sys;
use futures_util::StreamExt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// 全局 tokio 运行时：多线程小池，源码预取/CI 查询并发跑
/// Global tokio runtime: a small multi-thread pool for concurrent source
/// prefetch and CI queries
pub fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("ffbuild-net")
            .build()
            .expect("tokio runtime")
    })
}

/// 共享 reqwest 客户端：UA/超时/重定向；代理取 HTTPS_PROXY/HTTP_PROXY/
/// ALL_PROXY 环境变量（reqwest 默认行为）
/// Shared reqwest client: UA/timeouts/redirects; the proxy comes from
/// HTTPS_PROXY/HTTP_PROXY/ALL_PROXY (reqwest's default behaviour)
fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("ffbuild/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(3600))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("reqwest client")
    })
}

// ==================== HTTP ====================

/// 判断一次失败是否值得重试：传输错误/408/429/5xx 可重试，其余 4xx 终态
/// Whether a failure is worth retrying: transport errors and 408/429/5xx
/// are retryable; other 4xx are terminal
fn retryable(status: Option<reqwest::StatusCode>) -> bool {
    match status {
        None => true, // 连接/读写层错误 / transport-level error
        Some(s) => {
            s == reqwest::StatusCode::REQUEST_TIMEOUT
                || s == reqwest::StatusCode::TOO_MANY_REQUESTS
                || s.is_server_error()
        }
    }
}

/// 退避等待：5s×2^(n-1) 封顶 60s；带 Retry-After 时取两者较大（仍封顶）
/// Backoff: 5s*2^(n-1) capped at 60s; Retry-After wins when larger
fn backoff(attempt: u32, retry_after: Option<u64>) -> Duration {
    let base = (5u64 << (attempt - 1).min(4)).min(60);
    Duration::from_secs(retry_after.unwrap_or(0).max(base).min(60))
}

/// Retry-After 头解析（秒）/ parse a Retry-After header (seconds only)
fn retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
}

/// GET 一个 URL 并返回响应（重试 5 次，终态直接报错）
/// GET a URL and return the response (5 attempts, terminal statuses fail)
pub async fn get(url: &str, headers: &[(&str, &str)]) -> Result<reqwest::Response, String> {
    let mut last = String::new();
    for attempt in 1..=5 {
        let mut req = client().get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        match req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return Ok(resp);
                }
                let sc = resp.status();
                last = format!("GET {url}: HTTP {sc}");
                if !retryable(Some(sc)) {
                    return Err(last);
                }
                let ra = retry_after(&resp);
                log::warn(&format!("{last} — retrying ({attempt}/5)"));
                tokio::time::sleep(backoff(attempt, ra)).await;
            }
            Err(e) => {
                last = format!("GET {url}: {e}");
                if !retryable(e.status()) {
                    return Err(last);
                }
                log::warn(&format!("{last} — retrying ({attempt}/5)"));
                tokio::time::sleep(backoff(attempt, None)).await;
            }
        }
    }
    Err(last)
}

/// GET → UTF-8 文本 / GET a URL as text
pub async fn get_text(url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
    get(url, headers)
        .await?
        .text()
        .await
        .map_err(|e| format!("GET {url} body: {e}"))
}

/// GET → JSON（crate::json 解析）/ GET a URL as JSON (crate::json)
pub async fn get_json(url: &str, headers: &[(&str, &str)]) -> Result<crate::json::V, String> {
    let t = get_text(url, headers).await?;
    crate::json::parse(&t).map_err(|e| format!("GET {url}: {e}"))
}

/// 下载 URL 到文件：流式落盘；中断的半成品按 Range 续传；重试 5 次
/// Download a URL to a file: streamed to disk; a partial file resumes via
/// Range; retried 5 times
pub async fn download(url: &str, dest: &Path) -> Result<(), String> {
    let part = dest.with_extension(format!(
        "{}.part",
        dest.extension()
            .map(|x| x.to_string_lossy())
            .unwrap_or_default()
    ));
    fsx::install_d(dest.parent().unwrap())?;
    let mut last = String::new();
    for attempt in 1..=5 {
        let have = part.metadata().map(|m| m.len()).unwrap_or(0);
        let mut req = client().get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last = format!("GET {url}: {e}");
                if !retryable(e.status()) {
                    break;
                }
                log::warn(&format!("{last} — retrying ({attempt}/5)"));
                tokio::time::sleep(backoff(attempt, None)).await;
                continue;
            }
        };
        let sc = resp.status();
        // 续传被接受 → 追加；服务器忽略 Range → 从头重写
        // resume honoured → append; Range ignored → rewrite from scratch
        let (file, mut done) = if have > 0 && sc == reqwest::StatusCode::PARTIAL_CONTENT {
            (std::fs::OpenOptions::new().append(true).open(&part), have)
        } else if sc.is_success() {
            (std::fs::File::create(&part), 0)
        } else {
            last = format!("GET {url}: HTTP {sc}");
            if !retryable(Some(sc)) {
                return Err(last);
            }
            let ra = retry_after(&resp);
            log::warn(&format!("{last} — retrying ({attempt}/5)"));
            tokio::time::sleep(backoff(attempt, ra)).await;
            continue;
        };
        let mut f = file.map_err(|e| format!("open {part:?}: {e}"))?;
        let mut stream = resp.bytes_stream();
        let mut failed: Option<String> = None;
        let mut next_report = done + 32 * 1024 * 1024;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Err(e) = f.write_all(&bytes) {
                        failed = Some(format!("write {part:?}: {e}"));
                        break;
                    }
                    done += bytes.len() as u64;
                    if done >= next_report {
                        log::log(&format!("[download] {}: {}", url, sys::human_size(done)));
                        next_report += 64 * 1024 * 1024;
                    }
                }
                Err(e) => {
                    failed = Some(format!("GET {url} stream: {e}"));
                    break;
                }
            }
        }
        match failed {
            None => {
                f.flush().ok();
                std::fs::rename(&part, dest).map_err(|e| format!("mv {part:?} {dest:?}: {e}"))?;
                log::log(&format!(
                    "[download] {} -> {} ({})",
                    url,
                    dest.display(),
                    sys::human_size(done)
                ));
                return Ok(());
            }
            Some(e) => {
                last = e.clone();
                log::warn(&format!("{e} — retrying ({attempt}/5)"));
                tokio::time::sleep(backoff(attempt, None)).await;
            }
        }
    }
    Err(format!("download {url}: {last}"))
}

/// 同步包装：在共享运行时上跑 download
/// Sync wrapper: drive download on the shared runtime
pub fn download_sync(url: &str, dest: &Path) -> Result<(), String> {
    rt().block_on(download(url, dest))
}

/// 同步包装 get_json / sync wrapper for get_json
pub fn get_json_sync(url: &str, headers: &[(&str, &str)]) -> Result<crate::json::V, String> {
    rt().block_on(get_json(url, headers))
}

/// curl 下载兼容入口：等价物是 download_sync（重试/续传更好）
/// curl-download compat entry: the equivalent is download_sync
/// (better retry/resume)
pub fn curl(url: &str, out: &Path) -> Result<(), String> {
    download_sync(url, out)
}

// ==================== Git / ls-remote 助手 ====================

/// `git ls-remote <url> [patterns]` → (sha, ref) 行列表
/// List remote refs as (sha, ref) pairs
pub fn ls_remote(url: &str, patterns: &[&str]) -> Result<Vec<(String, String)>, String> {
    let mut args = vec!["ls-remote", url];
    args.extend_from_slice(patterns);
    let out = sys::capture("git", &args, &sys::env().work)?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.to_string()))
        })
        .collect())
}

/// 默认分支：(分支名, HEAD sha)——`git ls-remote --symref url HEAD`
/// Default branch: (name, HEAD sha) — `git ls-remote --symref url HEAD`
pub fn default_branch(url: &str) -> Result<(String, String), String> {
    let out = sys::capture(
        "git",
        &["ls-remote", "--symref", url, "HEAD"],
        &sys::env().work,
    )?;
    let mut branch = String::new();
    let mut sha = String::new();
    for l in out.lines() {
        if let Some(rest) = l.strip_prefix("ref: ") {
            // 形如 `ref: refs/heads/master\tHEAD`——先切 tab 再取末段
            // looks like `ref: refs/heads/master\tHEAD` — cut at the tab
            // first, then take the last path segment
            branch = rest
                .split('\t')
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string();
        } else if let Some(tok) = l.split_whitespace().next()
            && tok.chars().all(|c| c.is_ascii_hexdigit())
            && tok.len() == 40
        {
            sha = tok.to_string();
        }
    }
    if sha.is_empty() {
        return Err(format!("ls-remote {url}: HEAD not found"));
    }
    Ok((branch, sha))
}

/// 版本序比较键：非数字边界切分，数字段按值、其余按字典序
/// Version sort key: split on non-digit runs, numbers compare by value
fn vkey(tag: &str) -> Vec<u64> {
    tag.split(|c: char| !c.is_ascii_digit())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse().ok())
        .collect()
}

/// 远端最新 tag：全量 ls-remote --tags，取"像版本号"的最后一个（语义
/// 版本优先；实在没有版本样式的 tag 才取字典序最大者）
/// Latest remote tag: full ls-remote --tags, pick the last version-looking
/// one (semver-ish preferred; falls back to lexicographic max)
pub fn latest_tag(url: &str) -> Result<Option<String>, String> {
    // `--tags` 必须放在仓库名之前才是选项（放在其后会被当成 refs 模式，
    // 匹配不到任何内容静默返回空）/ `--tags` is only an option *before*
    // the repository argument — after it, git treats it as a ref pattern
    // and silently matches nothing
    let out = sys::capture("git", &["ls-remote", "--tags", url], &sys::env().work)?;
    let refs: Vec<(String, String)> = out
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.to_string()))
        })
        .collect();
    let mut tags: Vec<String> = refs
        .iter()
        .filter_map(|(_, r)| r.strip_prefix("refs/tags/").map(String::from))
        .filter(|t| !t.ends_with("^{}"))
        .collect();
    tags.sort();
    tags.dedup();
    let mut ver: Vec<&String> = tags.iter().filter(|t| !vkey(t).is_empty()).collect();
    ver.sort_by_key(|t| vkey(t));
    if let Some(t) = ver.last() {
        return Ok(Some((*t).clone()));
    }
    Ok(tags.last().cloned())
}

// ==================== GitHub / GitLab CI 查询 ====================

/// GitHub 请求头：Accept+版本+可选 token / GitHub request headers
fn gh_headers() -> Vec<(&'static str, String)> {
    let mut h = vec![
        ("Accept", "application/vnd.github+json".to_string()),
        ("X-GitHub-Api-Version", "2022-11-28".to_string()),
    ];
    // 两个变量名都认：workflow 步骤里给的是 GH_TOKEN，本地惯例是 GITHUB_TOKEN
    // accept both env names: the workflow exports GH_TOKEN, local convention is GITHUB_TOKEN
    for name in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(t) = std::env::var(name)
            && !t.is_empty()
        {
            h.push(("Authorization", format!("Bearer {t}")));
            break;
        }
    }
    h
}

/// 把一个动态头列表转成 get() 要的 &str 对 / adapt dynamic headers to the
/// (&str,&str) shape get() wants
fn hdr_pairs<'a>(v: &'a [(&'a str, String)]) -> Vec<(&'a str, &'a str)> {
    v.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

/// GitHub 某 commit 的 CI 是否绿：check-runs（结论）+ 旧版 status 合并判
/// 定。Some(true)=绿；Some(false)=有失败或仍在跑；None=没有任何 CI。
/// Is a GitHub commit CI-green: check-runs (conclusions) merged with legacy
/// statuses. Some(true)=green; Some(false)=a failure or still running;
/// None=no CI at all.
pub async fn github_ci_green(owner: &str, repo: &str, sha: &str) -> Result<Option<bool>, String> {
    let h = gh_headers();
    let hp = hdr_pairs(&h);
    // check-runs：GitHub Actions 等现代 check / check-runs: GitHub Actions
    // and other modern checks
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/commits/{sha}/check-runs?per_page=100"
    );
    let runs = get_json(&url, &hp).await?;
    let mut any = false;
    for r in runs.get("check_runs").arr() {
        any = true;
        let conclusion = r.get("conclusion").s();
        let status = r.get("status").s();
        let ok = matches!(conclusion, "success" | "neutral" | "skipped")
            || (status == "completed" && conclusion.is_empty());
        let bad = matches!(
            conclusion,
            "failure" | "cancelled" | "timed_out" | "action_required" | "startup_failure"
        );
        if bad || !ok {
            return Ok(Some(false)); // 失败或仍在跑 / failed or still running
        }
    }
    // 旧版 statuses / legacy commit statuses
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}/status");
    let st = get_json(&url, &hp).await?;
    for s in st.get("statuses").arr() {
        any = true;
        if s.get("state").s() != "success" {
            return Ok(Some(false));
        }
    }
    if any {
        return Ok(Some(true));
    }
    Ok(None)
}

/// 判定该 git 主机是不是 GitLab 实例（探测 /api/v4/version；已知清单
/// 快路径）/ decide whether a git host is a GitLab instance (probes
/// /api/v4/version; a known-host list shortcuts the probe)
fn is_gitlab(host: &str) -> bool {
    // 已知 GitLab 实例 / known GitLab instances
    const KNOWN: &[&str] = &[
        "gitlab.com",
        "code.videolan.org",
        "gitlab.freedesktop.org",
        "gitlab.gnome.org",
        "gitlab.xiph.org",
        "invent.kde.org",
        "salsa.debian.org",
    ];
    if KNOWN.contains(&host) {
        return true;
    }
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, bool>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(v) = cache.lock().unwrap().get(host) {
        return *v;
    }
    // 探测也带 token：要求鉴权的私有 GitLab 实例没它连 /version 都不给看
    // probe carries the token too: auth-requiring private GitLab instances
    // hide even /version without it
    let h = gl_headers();
    let hp = hdr_pairs(&h);
    let ok = get_json_sync(&format!("https://{host}/api/v4/version"), &hp).is_ok();
    cache.lock().unwrap().insert(host.to_string(), ok);
    ok
}

/// GitLab 请求头：可选 PRIVATE-TOKEN / GitLab request headers with an
/// optional PRIVATE-TOKEN
fn gl_headers() -> Vec<(&'static str, String)> {
    let mut h: Vec<(&'static str, String)> = vec![];
    // GITLAB_TOKEN（CI 注入）为主，GL_TOKEN 是 GitLab 生态的常见别名
    // GITLAB_TOKEN (injected by CI) first; GL_TOKEN is the common alias in
    // the GitLab ecosystem
    for name in ["GITLAB_TOKEN", "GL_TOKEN"] {
        if let Ok(t) = std::env::var(name)
            && !t.is_empty()
        {
            h.push(("PRIVATE-TOKEN", t));
            break;
        }
    }
    h
}

/// GitLab 某 commit 的 CI 是否绿：内部 pipelines + 外部 commit statuses。
/// Is a GitLab commit CI-green: internal pipelines plus external commit
/// statuses.
pub async fn gitlab_ci_green(host: &str, path: &str, sha: &str) -> Result<Option<bool>, String> {
    let h = gl_headers();
    let hp = hdr_pairs(&h);
    let enc = path.replace('/', "%2F");
    let base = format!("https://{host}/api/v4/projects/{enc}");
    let mut any = false;
    // 内部 pipelines / internal pipelines
    let url = format!("{base}/pipelines?sha={sha}&per_page=20");
    let pipes = get_json(&url, &hp).await?;
    for p in pipes.arr() {
        any = true;
        match p.get("status").s() {
            "success" => {}
            _ => return Ok(Some(false)),
        }
    }
    // 外部 CI 提交的 status（Jenkins/Buildkite 等）/ statuses reported by
    // external CIs
    let url = format!("{base}/repository/commits/{sha}/statuses?per_page=100");
    let sts = get_json(&url, &hp).await.unwrap_or(crate::json::V::Null);
    for s in sts.arr() {
        any = true;
        if s.get("status").s() != "success" {
            return Ok(Some(false));
        }
    }
    if any {
        return Ok(Some(true));
    }
    Ok(None)
}

/// 综合判定：<git url, commit sha> 的 CI 是否绿。
/// Some(true)/Some(false)/None(无 CI)。非 GitHub/GitLab 恒 None。
/// Combined verdict for <git url, commit sha>: Some(true)/Some(false)/
/// None (no CI). Non-GitHub/GitLab hosts always return None.
pub async fn ci_green(url: &str, sha: &str) -> Option<bool> {
    let host_path = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("git://")
        .trim_end_matches(".git")
        .trim_end_matches('/');
    let (host, path) = host_path.split_once('/')?;
    let host = if host == "www.github.com" {
        "github.com"
    } else {
        host
    };
    if host == "github.com" {
        let (owner, repo) = path.split_once('/')?;
        match github_ci_green(owner, repo, sha).await {
            Ok(v) => v,
            Err(e) => {
                log::warn(&format!("GitHub CI query failed for {url}@{sha}: {e}"));
                None
            }
        }
    } else if is_gitlab(host) {
        match gitlab_ci_green(host, path, sha).await {
            Ok(v) => v,
            Err(e) => {
                log::warn(&format!("GitLab CI query failed for {url}@{sha}: {e}"));
                None
            }
        }
    } else {
        None
    }
}

/// 同步包装 ci_green / sync wrapper for ci_green
pub fn ci_green_sync(url: &str, sha: &str) -> Option<bool> {
    rt().block_on(ci_green(url, sha))
}

// ==================== 源码获取原语 / source fetch primitives ====================

/// fetch_tar <url> [sha]：下载并校验解压到 <wd>/src（剥顶层目录），返回 src
/// fetch_tar <url> [sha]: download, verify and extract into <wd>/src
/// (top dir stripped); returns the src path
pub fn fetch_tar(wd: &Path, url: &str, sha: Option<&str>) -> Result<PathBuf, String> {
    let tar = wd.join("source.tar");
    download_sync(url, &tar)?;
    if let Some(want) = sha {
        let got = sys::sha256_file(&tar)?;
        if got != want {
            return Err(format!(
                "fetch_tar: sha256 mismatch for {url}\n  expected: {want}\n  got:      {got}"
            ));
        }
    }
    let src = wd.join("src");
    fsx::install_d(&src)?;
    // tar 自动识别 gz/bz2/xz 压缩格式 / tar auto-detects gz/bz2/xz
    sys::run_ok(
        "tar",
        &["-xf", "source.tar", "-C", "src", "--strip-components", "1"],
        wd,
    )?;
    Ok(src)
}

/// fetch_git <url> <ref> [sha]：浅克隆 tag/branch 到 <wd>/src
/// fetch_git <url> <ref> [sha]: shallow clone into <wd>/src
pub fn fetch_git(
    wd: &Path,
    url: &str,
    refname: &str,
    sha: Option<&str>,
) -> Result<PathBuf, String> {
    sys::run_ok(
        "git",
        &[
            "clone",
            "--depth",
            "1",
            "--branch",
            refname,
            "--recurse-submodules",
            "--shallow-submodules",
            url,
            "src",
        ],
        wd,
    )?;
    let src = wd.join("src");
    if let Some(want) = sha {
        let got = sys::capture(
            "git",
            &["-C", &src.to_string_lossy(), "rev-parse", "HEAD"],
            wd,
        )?;
        if got.trim() != want {
            return Err(format!(
                "fetch_git: ref {refname} of {url} resolved to {got}, expected {want}"
            ));
        }
    }
    Ok(src)
}

/// fetch_git_commit <url> <sha>：浅检出精确 commit 到 <wd>/src
/// fetch_git_commit <url> <sha>: shallow checkout of one exact commit
pub fn fetch_git_commit(wd: &Path, url: &str, sha: &str) -> Result<PathBuf, String> {
    fsx::rm_rf(&wd.join("src"));
    sys::run_ok("git", &["init", "src"], wd)?;
    let src = wd.join("src");
    sys::run_ok("git", &["remote", "add", "origin", url], &src)?;
    // 瞬时拉取失败重试 3 次（git.ffmpeg.org 周期性 502）
    // transient fetch failures retry 3x (git.ffmpeg.org 502s periodically)
    let mut ok = false;
    for i in 1..=3 {
        if sys::run("git", &["fetch", "--depth", "1", "origin", sha], &src)? == 0 {
            ok = true;
            break;
        }
        if i == 3 {
            return Err(format!("git fetch {url} {sha} failed after 3 attempts"));
        }
        std::thread::sleep(Duration::from_secs(10));
    }
    let _ = ok;
    sys::run_ok(
        "git",
        &["-c", "advice.detachedHead=false", "checkout", "FETCH_HEAD"],
        &src,
    )?;
    Ok(src)
}

// ==================== 测试 / tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// SourceForge 镜像跳转链路实测：downloads.sourceforge.net 会 302 到
    /// 镜像主机（或对某些 UA 回镜像选择页）。依赖外网、CI 里会拖慢且
    /// 不稳定——默认忽略，按需 `cargo test -- --ignored` 运行。
    /// SourceForge mirror-hop test: downloads.sourceforge.net 302s to a
    /// mirror host (or serves a mirror-picker page to some UAs).
    /// Network-dependent — ignored by default; run on demand with
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore = "hits the live SourceForge mirrors; run explicitly"]
    fn sourceforge_download() {
        let dest = std::env::temp_dir().join("lame-4.0-test.tar.gz");
        let _ = std::fs::remove_file(&dest);
        download_sync(
            "https://downloads.sourceforge.net/lame/lame/4.0/lame-4.0.tar.gz",
            &dest,
        )
        .expect("sourceforge download failed");
        let size = std::fs::metadata(&dest).unwrap().len();
        assert!(size > 1_000_000, "suspiciously small: {size}");
        // gzip 魔数 / gzip magic
        let head = std::fs::read(&dest).unwrap();
        assert_eq!(&head[..2], &[0x1f, 0x8b], "not a gzip stream");
        println!("OK: {size} bytes");
    }
}
