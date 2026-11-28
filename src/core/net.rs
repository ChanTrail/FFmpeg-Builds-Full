//! 网络层：reqwest(rustls) 异步下载（流式落盘、断点续传、重试退避）、
//! GitHub/GitLab CI 查询、git ls-remote 助手，以及 tar/git 源码获取原语。
//! Network layer: async reqwest(rustls) downloads (streamed to disk, resume,
//! retry with backoff), GitHub/GitLab CI queries, git ls-remote helpers and
//! the tar/git source-fetch primitives.
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

// ==================== HTTP 传输层 / transport ====================

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

/// 网络型 git 调用的统一 -c 前缀：http.lowSpeed* 让连接已建立但传输
/// stalled（<1KB/s 持续 60s）的调用 60s 内报错——git 默认对这类传输
/// 无限等待，CI 上表现为日志无声卡死；报错后交调用侧重试。对 https
/// 全部生效；git:// 原生协议不吃这套，好在全源清单里仅 librtmp 一家
/// 用 git://。maintenance.auto/gc.auto 关闭维护任务——一次性检出永远
/// 不需要 commit-graph/midx/增量 repack，新版 git 在 fetch/clone 尾部
/// 自动跑，几十个并发预取线程全程后台烧磁盘 I/O 拖慢整机。-c 经
/// GIT_CONFIG_PARAMETERS 传给子 git 进程，clone 递归拉起的子模块
/// fetch 同样生效
/// Shared -c prefix for network git calls: http.lowSpeed* fails a
/// connected-but-stalled transfer (<1KB/s for 60s) inside 60s — git's
/// default waits forever on those, which on CI reads as a silent log
/// freeze; the caller's retry layer then takes over. It covers every
/// https transport; the git:// protocol ignores it, but only librtmp
/// in the whole source map uses git://. maintenance.auto/gc.auto kill
/// repo maintenance — throwaway checkouts never need commit-graph/
/// midx/incremental-repack, yet modern git fires them at the tail of
/// every fetch/clone and dozens of concurrent prefetch threads burn
/// disk I/O all build long. -c propagates to child git processes via
/// GIT_CONFIG_PARAMETERS, covering the submodule fetches spawned by
/// clone
pub(crate) const GIT_NET: &[&str] = &[
    "-c",
    "http.lowSpeedLimit=1024",
    "-c",
    "http.lowSpeedTime=60",
    "-c",
    "maintenance.auto=false",
    "-c",
    "gc.auto=0",
];

/// git 传输重试的退避序列：网关 504/拥塞在 ~10s 尺度上很少自愈，
/// 15/30/60/90s 递增才躲得过高峰（实证：freedesktop 三连炸全死在
/// 固定 10s 窗口里）/ backoff ladder for git transport retries:
/// gateway 504s / congestion rarely heal on a ~10s scale — the
/// 15/30/60/90s ramp dodges the busy window (empirical: three flat-10s
/// attempts all died inside one freedesktop burst)
fn git_retry_delay(attempt: u32) -> Duration {
    Duration::from_secs([15, 30, 60, 90][(attempt as usize).min(3)])
}

/// git 网络读操作统一入口：GIT_NET 传输超时 + 5 次递增退避重试
/// （ls-remote / 轻量探测用）/ shared entry for network git reads:
/// GIT_NET transport timeouts + 5 attempts on a rising backoff
/// (ls-remote / light probes)
fn git_capture(args: &[&str]) -> Result<String, String> {
    let mut a: Vec<&str> = GIT_NET.to_vec();
    a.extend_from_slice(args);
    let mut last = String::new();
    for i in 1..=5 {
        match sys::capture("git", &a, &sys::env().work) {
            Ok(o) => return Ok(o),
            Err(e) => {
                last = e;
                if i < 5 {
                    std::thread::sleep(git_retry_delay(i));
                }
            }
        }
    }
    Err(last)
}

/// 同 git_capture，面向无输出命令（clone/fetch）。clean_between 是
/// 重试前要清掉的半成品路径（clone 拒绝非空目录，fetch 不需要）/
/// same as git_capture for output-less commands (clone/fetch), wd as
/// cwd. clean_between is a partial-result path to wipe before each
/// retry (clone refuses a non-empty dir; fetch needs none)
pub(crate) fn git_run(
    args: &[&str],
    wd: &Path,
    clean_between: Option<&Path>,
) -> Result<(), String> {
    let mut a: Vec<&str> = GIT_NET.to_vec();
    a.extend_from_slice(args);
    let mut last = String::new();
    // 同 git_capture 的 5 次递增退避——clone/fetch 撞 504 是同一类
    // 瞬时网关拥塞 / same rising 5-attempt backoff as git_capture —
    // clone/fetch hitting 504 is the same transient gateway congestion
    for i in 1..=5 {
        match sys::run("git", &a, wd) {
            Ok(0) => return Ok(()),
            Ok(c) => last = format!("exited {c}"),
            Err(e) => last = e,
        }
        if i < 5 {
            if let Some(p) = clean_between {
                fsx::rm_rf(p);
            }
            std::thread::sleep(git_retry_delay(i));
        }
    }
    Err(format!(
        "git {} failed: {last}",
        args.first().unwrap_or(&"?")
    ))
}

/// `git ls-remote <url> [patterns]` → (sha, ref) 行列表
/// List remote refs as (sha, ref) pairs
pub fn ls_remote(url: &str, patterns: &[&str]) -> Result<Vec<(String, String)>, String> {
    let mut args = vec!["ls-remote", url];
    args.extend_from_slice(patterns);
    let out = git_capture(&args)?;
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
    let out = git_capture(&["ls-remote", "--symref", url, "HEAD"])?;
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

/// "版本样式"tag 判据：vkey 至少两个数字段——点式（v2.13.3）与
/// 横杠式（VER-2-13-3）同收。单字段的修订号 tag（r2772-MT ->
/// [2772]）与裸名（freetype2 -> [2]、import/start -> []）出局。
/// 日期戳形状（2013-11-24 -> [2013,11,24]）另加显式排除：年份形
/// 首段 + 月日范围在 vkey 首元素压过任何真版本，会把回退检出骗到
/// 化石快照（freetype 实证：VER-* 横杠版号全被点式判据拒收，ver
/// 空池后 tags.last() 捞到字典序最大的 `start`——CVS 化石 tag，
/// 树里没有 meson.build，配方当场暴毙）/
/// "Version-looking" tag test: at least two numeric vkey fields —
/// dotted (v2.13.3) and dashed (VER-2-13-3) forms alike. Single-field
/// revision tags (r2772-MT -> [2772]) and bare names (freetype2 ->
/// [2], import/start -> []) are out. Datestamp shapes (2013-11-24 ->
/// [2013,11,24]) get an explicit exclusion: a year-shaped head +
/// month/day range outranks every real version on vkey[0] and would
/// seduce the fallback into a fossil checkout (freetype evidence: the
/// dot-only test rejected every dashed VER-* tag, and with `ver`
/// empty, tags.last() fetched `start` — a CVS fossil whose tree has
/// no meson.build at all, killing the recipe on the spot)
/// 横杠日期子串扫描：tag 任意位置嵌着 YYYY-MM[-DD]（年 1900-2999、
/// 月 01-12、日 01-31）即为日期戳——嵌入式形态（3gpp-2021-10-15-5）
/// 的首个数字段不是年份，位置化 vkey 判据看不见它，会把化石快照
/// 推赢真版本（实证：aom 回退抓 3gpp-2021-10-15-5 而非 v3.15.x）/
/// dashed-date substring scan: a YYYY-MM[-DD] embedded anywhere (year
/// 1900-2999, month 01-12, day 01-31) makes the tag a datestamp — the
/// embedded form (3gpp-2021-10-15-5) hides from the positional vkey
/// test because its first numeric field isn't the year, letting fossil
/// snapshots outrank real versions (empirical: aom's fallback fetched
/// 3gpp-2021-10-15-5 instead of v3.15.x)
/// Params：
/// - t: &str —— tag 名 / the tag name
fn has_dashed_date(t: &str) -> bool {
    let b = t.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let num = |i: usize, n: usize| -> Option<u64> {
        (i..i + n).try_fold(0u64, |acc, j| {
            if digit(j) {
                Some(acc * 10 + u64::from(b[j] - b'0'))
            } else {
                None
            }
        })
    };
    for i in 0..b.len() {
        // 年份段起点：4 位数字且不在更长数字串中间 / year-field start:
        // 4 digits not embedded in a longer digit run
        if !digit(i) || digit(i.wrapping_sub(1)) && i > 0 {
            continue;
        }
        let Some(year) = num(i, 4) else { continue };
        if !(1900..=2999).contains(&year) {
            continue;
        }
        if b.get(i + 4) != Some(&b'-') {
            continue;
        }
        let Some(month) = num(i + 5, 2) else { continue };
        if !(1..=12).contains(&month) {
            continue;
        }
        // 月已坐实；日段在场则同样校验 / the month alone convicts; the
        // day field gets the same check when present
        if b.get(i + 7) == Some(&b'-')
            && let Some(day) = num(i + 8, 2)
            && !(1..=31).contains(&day)
        {
            continue;
        }
        return true;
    }
    false
}

/// 发布形结构判据：剥掉前导非数字缀（v/release-/libgcrypt-/ww 等）后，
/// 余部必须是一条纯数字点/横杠链加可选预发布缀。vkey 只提取数字段，
/// 对数字间/数字后的字母垃圾完全隐形——管家标记 tag 凭高数字段碾压
/// 真版本（实证：openssl 回退抓 `4.0-POST-CLANG-FORMAT-WEBKIT` [4,0]
/// 压过 openssl-3.x 全系）。ww53.4-video 式的内部构建标签同属此类——
/// 原"降级为兜底"的处置升级为整体出局：它们缺席时回退落到
/// tags.last()/分支 HEAD，与日期版号的误伤同级，可接受 /
/// release-shape structural test: after stripping the leading non-digit
/// prefix (v/release-/libgcrypt-/ww etc.) the remainder must be a pure
/// numeric dot/dash chain plus an optional pre-release suffix. vkey
/// extracts only the numeric fields and is completely blind to alpha
/// junk between or after them — housekeeping marker tags outrank real
/// releases on their high numeric fields (empirical: openssl's fallback
/// fetched `4.0-POST-CLANG-FORMAT-WEBKIT` [4,0] over the whole
/// openssl-3.x series). ww53.4-video-style internal build labels are
/// the same class — the former "demote to last-resort" treatment is
/// upgraded to full exclusion: in their absence the fallback lands on
/// tags.last()/branch HEAD, the same accepted-collateral class as
/// date-versioned tags
/// Params：
/// - t: &str —— tag 名 / the tag name
fn has_release_shape(t: &str) -> bool {
    let b = t.as_bytes();
    let n = b.len();
    // 剥前导非数字缀 / strip the leading non-digit prefix
    let mut i = 0;
    while i < n && !b[i].is_ascii_digit() {
        i += 1;
    }
    if i == n {
        return false; // 全串无数字 / no digits at all
    }
    // 数字链核：首段数字 + 任意个 (分隔符+数字) 组 / numeric chain core:
    // a first digit run plus any number of (separator + digits) groups
    let mut j = i;
    while j < n && b[j].is_ascii_digit() {
        j += 1;
    }
    loop {
        if j >= n {
            return true; // 纯数字链收尾 / the chain ends clean
        }
        if matches!(b[j], b'.' | b'-' | b'_') && j + 1 < n && b[j + 1].is_ascii_digit() {
            j += 1;
            while j < n && b[j].is_ascii_digit() {
                j += 1;
            }
            continue;
        }
        break;
    }
    // 余部：空或预发布缀（rc/alpha/beta/pre + 可选数字点尾）——其余
    // 一切皆为管家垃圾 / the remainder: empty or a pre-release suffix
    // (rc/alpha/beta/pre + an optional digit/dot tail) — everything
    // else is housekeeping junk
    let rest = t[j..].to_lowercase();
    if rest.is_empty() {
        return true;
    }
    let rest = rest.strip_prefix(['-', '_', '.']).unwrap_or(rest.as_str());
    ["rc", "alpha", "beta", "pre"].iter().any(|p| {
        // 缀词后只允许数字与点（空尾也算——`v1.0-rc` 无编号形态）/
        // only digits and dots may follow the suffix word (an empty
        // tail counts too — the numberless `v1.0-rc` form)
        rest.strip_prefix(p)
            .is_some_and(|tail| tail.chars().all(|c| c.is_ascii_digit() || c == '.'))
    })
}

fn is_version_tag(t: &str) -> bool {
    let v = vkey(t);
    if v.len() < 2 {
        return false;
    }
    // YYYY-MM[-DD] 日期形：首段四位年份 + 二段 1-12 + 三段（在场时）
    // 1-31。合法日期版号（v2024.05.1 系）同被误伤——但它们缺席时
    // 回退落到 tags.last()/分支 HEAD，风险面小于化石胜出 /
    // YYYY-MM[-DD] date shape: 4-digit year head + month 1-12 + day
    // 1-31 when present. Legit date-versioned tags (v2024.05.1 kin)
    // get caught too — but their absence falls back to tags.last()/
    // branch HEAD, a smaller hazard than a datestamp winning
    if (1900..=2999).contains(&v[0])
        && (1..=12).contains(&v[1])
        && (v.len() == 2 || (1..=31).contains(&v[2]))
    {
        return false;
    }
    if has_dashed_date(t) {
        return false;
    }
    if !has_release_shape(t) {
        return false;
    }
    true
}

/// 预发布 tag？——`-rc/-alpha/-beta/-pre` 分隔缀形式（大小写不敏感）。
/// 词中子串不算：src 含 rc、archive 含 rc 都不是预发布标记 / is the
/// tag a pre-release — separated `-rc/-alpha/-beta/-pre` suffix forms
/// (case-insensitive); in-word substrings don't count: src carries rc,
/// archive carries rc, neither is a pre-release marker
/// Params：
/// - t: &str —— tag 名 / the tag name
fn is_prerelease(t: &str) -> bool {
    let l = t.to_lowercase();
    // `.pre/_pre` 同列：intel-mediasdk-18.3.pre1 式（实证：MediaSDK 仓
    // pre 缀未检出时 [18,3,1] vkey 压过正式版 [18,3,0]）/ `.pre/_pre`
    // belong here too — the intel-mediasdk-18.3.pre1 form (empirical:
    // undetected pre suffixes in the MediaSDK repo outranked the final
    // [18,3,0] with their [18,3,1] vkey)
    [
        "-rc", "_rc", ".rc", "-alpha", "-beta", "-pre", ".pre", "_pre",
    ]
    .iter()
    .any(|p| l.contains(p))
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
    let out = git_capture(&["ls-remote", "--tags", url])?;
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
    let mut ver: Vec<&String> = tags.iter().filter(|t| is_version_tag(t)).collect();
    // 预发布出局层：有稳定版时 rc/alpha/beta 不得参选——vkey 排序里
    // [3,15,3,1] 长于 [3,15,3] 反而更大，不过滤 rc1 会永远压过它的
    // 正式版（实证：aom 回退抓 v3.15.3-rc1 而非 v3.15.3）。全是预发布
    // 的仓库则原样保留——有 rc 吃好过饿死 / the pre-release exclusion
    // layer: with any stable tag present, rc/alpha/beta tags cannot
    // compete — in vkey ordering [3,15,3,1] outranks [3,15,3] by being
    // longer, so unfiltered rc1 forever shadows its own final
    // (empirical: aom's fallback fetched v3.15.3-rc1 instead of
    // v3.15.3). Repos with nothing but pre-releases keep them all —
    // rc food beats starvation
    let stable: Vec<&String> = ver.iter().copied().filter(|t| !is_prerelease(t)).collect();
    if !stable.is_empty() {
        ver = stable;
    }
    ver.sort_by_key(|t| vkey(t));
    // 纯 vkey 最大者当选——曾有的"裸版号优先"层已拆除：它唯一的服务
    // 对象 ww 系内部标签由 has_release_shape 整体出局，而该层在
    // MediaSDK 上反噬真版本（实证：远古裸版 1.x 压过嵌入式真发布系
    // intel-mediasdk-18/19/2x——嵌入式名称恰是仓主的发布系本身，
    // libgcrypt/samba/openssl 诸仓同构）/ the pure vkey max wins — the
    // former "bare versions first" layer is dismantled: its sole raison
    // d'être, ww-style internal labels, is now excluded wholesale by
    // has_release_shape, and the layer backfired on MediaSDK
    // (empirical: the ancient bare 1.x series outranked the embedded
    // intel-mediasdk-18/19/2x line — embedded names being the
    // maintainer's release series itself, as in libgcrypt/samba/openssl)
    if let Some(t) = ver.last().copied() {
        return Ok(Some(t.clone()));
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
/// 快路径）。必须 async——同步版用 get_json_sync 会在调用方 block_on
/// 里再压一层 block_on，直接 "Cannot start a runtime from within a
/// runtime" panic。
/// decide whether a git host is a GitLab instance (probes
/// /api/v4/version; a known-host list shortcuts the probe). Must stay
/// async — a sync version via get_json_sync nests a second block_on
/// inside the caller's block_on and panics with "Cannot start a
/// runtime from within a runtime".
async fn is_gitlab(host: &str) -> bool {
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
    let ok = get_json(&format!("https://{host}/api/v4/version"), &hp)
        .await
        .is_ok();
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
    } else if is_gitlab(host).await {
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

// ==================== commit 日期探针 / commit date probes ====================

/// ISO8601（YYYY-MM-DDTHH:MM:SS 前缀）转 unix 秒——时区修正忽略：
/// ±小时级误差对四年量级的停滞判定无意义 / ISO8601 (the
/// YYYY-MM-DDTHH:MM:SS prefix) to unix seconds — timezone correction
/// deliberately ignored: hour-scale error is meaningless against a
/// four-year staleness horizon
/// Params：
/// - s: &str —— ISO 日期串 / the ISO date string
fn iso_to_unix(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let num = |a: usize, b: usize| s[a..b].parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    // days-from-civil（Howard Hinnant 算法）/ days-from-civil
    // (Howard Hinnant's algorithm)
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + sec)
}

/// git 兜底探针：浅抓该 sha 后读 committer 时间戳——任何宿主通用，
/// 代价是一次 depth-1 传输 / git fallback probe: shallow-fetch the
/// sha and read its committer timestamp — universal across hosts at
/// the cost of one depth-1 transfer
/// Params：
/// - url: &str —— 仓 URL / the repo URL
/// - sha: &str —— 目标 commit / the target commit
fn commit_date_git(url: &str, sha: &str) -> Option<i64> {
    let dir = std::env::temp_dir().join(format!(
        "ff-dateprobe-{}-{}",
        std::process::id(),
        &sha[..12.min(sha.len())]
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let out = (|| {
        git_run(&["init", "-q"], &dir, None).ok()?;
        git_run(
            &[
                "fetch",
                "-q",
                "--depth",
                "1",
                "--filter=blob:none",
                url,
                sha,
            ],
            &dir,
            None,
        )
        .ok()?;
        git_capture(&[
            "-C",
            dir.to_str()?,
            "log",
            "-1",
            "--format=%ct",
            "FETCH_HEAD",
        ])
        .ok()
    })();
    let _ = std::fs::remove_dir_all(&dir);
    out?.trim().parse().ok()
}

/// commit 日期探针：宿主 API 快路径（github/gitlab 系），缺席或不
/// 支持时退化 git 浅抓——上游停滞判定的唯一事实来源 / commit date
/// probe: host API fast paths (github/gitlab family), degrading to a
/// git shallow fetch where absent or unsupported — the single source
/// of truth for upstream staleness judgment
/// Params：
/// - url: &str —— 仓 URL / the repo URL
/// - sha: &str —— 目标 commit / the target commit
pub fn commit_date(url: &str, sha: &str) -> Option<i64> {
    let rest = url
        .trim_end_matches('/')
        .strip_prefix("https://")
        .or_else(|| url.trim_end_matches('/').strip_prefix("git://"))?;
    let (host, path) = rest.split_once('/')?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let found = rt().block_on(async {
        if host == "github.com" {
            let mut it = path.split('/');
            let (owner, repo) = (it.next()?, it.next()?);
            let h = gh_headers();
            let hp = hdr_pairs(&h);
            let api = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
            let j = get_json(&api, &hp).await.ok()?;
            return iso_to_unix(j.get("commit").get("committer").get("date").s());
        }
        if is_gitlab(host).await {
            let h = gl_headers();
            let hp = hdr_pairs(&h);
            // 路径需 %2F 编码 / the path needs %2F encoding
            let enc = path.replace('/', "%2F");
            let api = format!("https://{host}/api/v4/projects/{enc}/repository/commits/{sha}");
            let j = get_json(&api, &hp).await.ok()?;
            return iso_to_unix(j.get("committed_date").s());
        }
        None
    });
    found.or_else(|| commit_date_git(url, sha))
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
    // GIT_NET 统一 -c：传输停滞 60s 判死 + 关维护任务；半成品 src
    // 在重试前清掉（clone 拒绝非空目录）/ GIT_NET's unified -c:
    // stalled transfers die in 60s + maintenance off; the partial src
    // is wiped between retries (clone refuses a non-empty dir)
    git_run(
        &[
            "clone",
            "-q",
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
        Some(&wd.join("src")),
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
    sys::run_ok("git", &["init", "-q", "src"], wd)?;
    let src = wd.join("src");
    sys::run_ok("git", &["remote", "add", "origin", url], &src)?;
    // 瞬时拉取失败重试 3 次（git.ffmpeg.org 周期性 502）——git_run
    // 内置重试；fetch 增量续传无需清理 / transient fetch failures
    // retry 3x inside git_run (git.ffmpeg.org 502s periodically);
    // fetch resumes incrementally, nothing to clean
    git_run(&["fetch", "-q", "--depth", "1", "origin", sha], &src, None)
        .map_err(|e| format!("git fetch {url} {sha} {e}"))?;
    sys::run_ok(
        "git",
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "-q",
            "FETCH_HEAD",
        ],
        &src,
    )?;
    Ok(src)
}

// ==================== 单元测试 / unit tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_tag_basics() {
        // 点式与横杠版号同收 / dotted and dashed versions alike
        assert!(is_version_tag("v2.13.3"));
        assert!(is_version_tag("VER-2-13-3"));
        assert!(is_version_tag("1.2.3"));
        assert!(is_version_tag("v3.15.3-rc1")); // 预发布也是版本——出局在 latest_tag 的过滤层 / pre-releases are versions too — exclusion is latest_tag's filter layer
        // 单字段修订号与裸名出局 / single-field revisions and bare names out
        assert!(!is_version_tag("r2772-MT"));
        assert!(!is_version_tag("freetype2"));
        assert!(!is_version_tag("start"));
        assert!(!is_version_tag("import"));
    }

    #[test]
    fn datestamp_exclusion() {
        // 位置形日期 / positional date shapes
        assert!(!is_version_tag("2013-11-24"));
        assert!(!is_version_tag("v2024.05.1"));
        // 嵌入式横杠日期（3gpp 化石实证）/ embedded dashed dates
        // (the 3gpp fossil evidence)
        assert!(!is_version_tag("3gpp-2021-10-15-5"));
        assert!(!is_version_tag("3gpp-2021-10-15"));
        assert!(!is_version_tag("foo-2024-01"));
        // 非日期不误伤：年份段无月、月超界、纯版本 / non-dates
        // untouched: year without month, out-of-range month, plain versions
        assert!(is_version_tag("v2.13.3"));
        assert!(is_version_tag("release-2.32.10"));
    }

    #[test]
    fn release_shape_exclusion() {
        // 管家标记化石（openssl 实证）/ housekeeping marker fossils
        // (openssl evidence)
        assert!(!is_version_tag("4.0-POST-CLANG-FORMAT-WEBKIT"));
        // 内部构建标签（ww 系——整体出局升级）/ internal build labels
        // (ww kin — the wholesale-exclusion upgrade)
        assert!(!is_version_tag("ww53.4-video"));
        // 数字间夹字母垃圾 / alpha junk between numerics
        assert!(!is_version_tag("3gpp2-1release"));
        // 合法形态不误伤 / legitimate shapes untouched
        assert!(is_version_tag("openssl-3.5.0"));
        assert!(is_version_tag("libgcrypt-1.12.4"));
        assert!(is_version_tag("vid.obe.1.7.0"));
        assert!(is_version_tag("n13.1.15.0"));
        assert!(is_version_tag("VER-2-15-0"));
        assert!(is_version_tag("v2023.4.0"));
        assert!(is_version_tag("v3.15.3-rc1"));
        assert!(is_version_tag("v1.0-rc"));
        assert!(is_version_tag("v2.0-rc.1"));
    }

    #[test]
    fn dashed_date_scanner() {
        assert!(has_dashed_date("3gpp-2021-10-15-5"));
        assert!(has_dashed_date("x-1999-12-31"));
        assert!(has_dashed_date("x-2024-01"));
        assert!(!has_dashed_date("v2.13.3"));
        assert!(!has_dashed_date("x-2024-13-01")); // 月 13 非日期 / month 13 is no date
        assert!(!has_dashed_date("2024")); // 裸年份不够 / a bare year is not enough
        assert!(!has_dashed_date("x1-20240-10")); // 年份嵌在更长数字串里 / year inside a longer digit run
    }

    #[test]
    fn prerelease_detection() {
        assert!(is_prerelease("v3.15.3-rc1"));
        assert!(is_prerelease("v1.0-alpha"));
        assert!(is_prerelease("v1.0-BETA2"));
        assert!(is_prerelease("v2.0-pre"));
        assert!(is_prerelease("v2.0-rc.1"));
        // 点式 pre 缀（MediaSDK 实证）/ dotted pre suffixes (MediaSDK evidence)
        assert!(is_prerelease("intel-mediasdk-18.3.pre1"));
        assert!(is_prerelease("v1.0.pre2"));
        assert!(!is_prerelease("v3.15.3"));
        assert!(!is_prerelease("src-1.0")); // 词中 rc 不算 / in-word rc doesn't count
        assert!(!is_prerelease("v1.0"));
    }

    #[test]
    fn vkey_ordering() {
        assert!(vkey("v3.15.3") < vkey("v3.15.3.1"));
        assert!(vkey("v3.9.1") < vkey("v3.15.0"));
        assert!(vkey("VER-2-13-3") == vkey("v2.13.3"));
    }
}
