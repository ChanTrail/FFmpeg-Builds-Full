//! Windows deps overlay：deps release 的恢复/发布与产物修正。
//! 库前缀构建一次被每轮复用（deps-<platform>-*.tar.zst），pacman 拥有的
//! 路径永远不上 overlay 的车——恢复时跳过、发布时剔除。deps_fixup 在
//! pacman 新装的文件上每轮重新推导全部 .pc/归档补丁。
//! Windows deps overlay: restore/publish of the deps release plus the
//! fixups. The library prefix is built once and reused every run
//! (deps-<platform>-*.tar.zst); pacman-owned paths never ride the overlay —
//! skipped at restore, dropped from the publish manifest. deps_fixup
//! re-derives every .pc/archive patch on pacman's fresh files each run.

use crate::sys::{self, Env};
use crate::{deps, fsx, log, pkgcfg, recipes};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// deps 签名：包表 + 全部配方源码 + 引擎版本 + MSYSTEM + 修订号。
/// 输入变了前缀就得重验 extras（恢复方清标记、配方按 .ok 哈希自选）。
/// The deps signature: package table + every recipe source + the engine
/// marker + MSYSTEM + a revision tag. Changed inputs mean the prefix must
/// re-verify extras (restore clears the markers; recipes self-select via
/// their .ok hashes).
fn sig() -> String {
    let e = sys::env();
    let mut srcs = String::new();
    for r in recipes::table() {
        srcs.push_str(r.name);
        srcs.push('\n');
        srcs.push_str(r.src);
        srcs.push('\n');
    }
    deps::key(&[
        &crate::manifest::packages::file_path("windows.yaml").to_string_lossy(),
        &srcs,
        &format!("{}-engine", env!("CARGO_PKG_VERSION")),
        &e.msystem,
        "cv2",
    ])
}

/// 前缀内任何已装 pacman 包拥有的文件（./relative 形式）——include/ 整树
/// 除外（那棵树归 extras；aribcc_config.h 唯一例外回列，它是打补丁而非
/// 替换的头文件）。vendored 清单 lib/.vendored-* 里的路径名义归 pacman、
/// 实为 extras 产物，从清单剔除（不然自建 glib 永远上不了车）。
/// Every pacman-owned file under the prefix in ./relative form — except
/// the include/ tree (extras own that; aribcc_config.h is the single
/// re-listed exception, a header we PATCH not replace). Paths in the
/// vendored manifests lib/.vendored-* nominally belong to pacman but are
/// extras output — dropped so self-built glib can ride the overlay.
fn pacman_files() -> Vec<String> {
    let e = sys::env();
    let pfx = format!("{}/", e.env_prefix);
    let owned: HashSet<String> = sys::capture("pacman", &["-Qlq"], &e.work)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix(&pfx).map(|r| format!("./{r}")))
        .collect();
    // vendored 豁免集 / vendored exemption set
    let mut vlist: HashSet<String> = HashSet::new();
    let libdir = e.prefix_win.join("lib");
    if let Ok(rd) = std::fs::read_dir(&libdir) {
        for ent in rd.flatten() {
            if ent.file_name().to_string_lossy().starts_with(".vendored-") {
                for l in fsx::read(&ent.path()).lines() {
                    let l = l.trim();
                    if l.starts_with("./") && !l.contains("..") {
                        vlist.insert(l.to_string());
                    }
                }
            }
        }
    }
    let mut out: Vec<String> = owned
        .iter()
        .filter(|f| {
            (!f.starts_with("./include/") || f.as_str() == "./include/aribcaption/aribcc_config.h")
                && !vlist.contains(*f)
        })
        .cloned()
        .collect();
    out.sort();
    out
}

/// 前缀文件清单（./relative 排序形式，deps-before/after 快照用）
/// Sorted ./relative file listing of the prefix (deps-before/after snaps)
fn prefix_listing(e: &Env) -> Vec<String> {
    let mut v: Vec<String> = fsx::walk(&e.prefix_win)
        .iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(&e.prefix_win).ok()?;
            Some(format!("./{}", rel.to_string_lossy().replace('\\', "/")))
        })
        .collect();
    v.sort();
    v
}

/// shaderc_combined.pc → shaderc.pc 遮蔽 + CXXRT；spirv-cross-c-shared.pc
/// 静态版手写（pacman 那份指共享库，会让 libplacebo 门槛每轮重跑）。
/// Shadow shaderc_combined.pc -> shaderc.pc + CXXRT; hand-write the static
/// spirv-cross-c-shared.pc (pacman's points at the shared lib and would
/// refail libplacebo's static gate every run).
fn shaderc_shadow(e: &Env) {
    let pcdir = e.prefix_win.join("lib/pkgconfig");
    let comb = pcdir.join("shaderc_combined.pc");
    let shp = pcdir.join("shaderc.pc");
    if comb.is_file() {
        let _ = std::fs::copy(&comb, &shp);
        if !pkgcfg::token_present(&shp, &e.cxxrt) {
            pkgcfg::libs_private("shaderc", &[&e.cxxrt]);
        }
    }
    if pkgcfg::exists("spirv-cross-c-shared") {
        let scv = {
            let v = pkgcfg::modversion("spirv-cross-c-shared");
            if v.is_empty() {
                "0.29.0".to_string()
            } else {
                v
            }
        };
        let _ = fsx::write(
            &pcdir.join("spirv-cross-c-shared.pc"),
            &format!(
                "prefix={}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include/spirv_cross\n\nName: spirv-cross-c-shared\nDescription: C API for SPIRV-Cross (static)\nVersion: {scv}\nLibs: -L${{libdir}} -lspirv-cross-c -lspirv-cross-glsl -lspirv-cross-hlsl -lspirv-cross-reflect -lspirv-cross-msl -lspirv-cross-util -lspirv-cross-core {}\nCflags: -I${{includedir}}\n",
                e.prefix, e.cxxrt
            ),
        );
    }
}

/// 恢复 deps overlay：取最新产物 → vendored 清单预提取 → 扣 pacman 拥有
/// 文件解压 → 签名不符清 extras 标记 → shaderc 遮蔽 → 记前缀快照。
/// Restore the deps overlay: fetch the newest asset -> pre-extract the
/// vendored manifests -> unpack skipping pacman-owned files -> clear the
/// extras markers when the signature differs -> shaderc shadowing ->
/// snapshot the prefix for the publish delta.
pub fn restore() -> Result<(), String> {
    let e = sys::env();
    let sig = sig();
    fsx::write(&e.work.join("deps-sig"), &sig)?;
    let ddir = e.work.join("deps");
    if let Ok(asset) = deps::fetch(&e.plat(), &ddir) {
        fsx::write(&e.work.join("deps-restored"), &asset)?;
        log::group_begin(&format!("Restore deps overlay ({asset})"));
        let apath = ddir.join(&asset);
        let aposix = sys::win_to_posix(&apath, &e.msys2_root);
        // 记住 overlay 内容：发布清单是它 + 本轮 extras 新增之并集
        // remember the overlay's contents — the publish manifest is the
        // union of this and what the extras add now
        let oldman =
            sys::capture("tar", &["-I", "zstd", "-tf", &aposix], &e.work).unwrap_or_default();
        fsx::write(&e.work.join("deps-old-manifest.txt"), &oldman)?;
        // vendored 清单先单独落地——pacman_files 要靠它们豁免 / vendored
        // manifests first — pacman_files needs them for the exemptions
        let _ = sys::bash(
            &format!(
                "tar -C '{}' -I zstd -xf '{}' --wildcards './lib/.vendored-*' 2>/dev/null || true",
                e.env_prefix, aposix
            ),
            &e.work,
        );
        // pacman 拥有的成员整批跳过（overlay 副本可能陈旧/损坏）
        // skip every pacman-owned member (the overlay copy may be stale)
        let owned: HashSet<String> = pacman_files().into_iter().collect();
        fsx::write(
            &e.work.join("pacman-files.txt"),
            &owned.iter().cloned().collect::<Vec<_>>().join("\n"),
        )?;
        let skip: Vec<&str> = oldman.lines().filter(|l| owned.contains(*l)).collect();
        fsx::write(&e.work.join("deps-skip.txt"), &skip.join("\n"))?;
        let skiposix = sys::win_to_posix(&e.work.join("deps-skip.txt"), &e.msys2_root);
        sys::bash_ok(
            &format!(
                "tar -C '{}' -I zstd -xf '{}' --exclude-from='{}'",
                e.env_prefix, aposix, skiposix
            ),
            &e.work,
        )?;
        fsx::rm_rf(&ddir);
        // 输入集不符 → extras 重跑；.ok 标记保留（带源哈希自选）
        // different inputs -> extras re-check; .ok markers stay (they carry
        // the per-recipe source hash and self-select)
        let sigfile = e.prefix_win.join(".deps-sig");
        if fsx::read(&sigfile).trim() != sig {
            log::log("deps overlay was built for different inputs; extras will re-check");
            let exdir = e.prefix_win.join(".extras");
            for f in ["done", "status.txt", "flags.txt"] {
                fsx::rm_rf(&exdir.join(f));
            }
        }
        log::group_end();
    }
    fsx::write(&e.prefix_win.join(".deps-sig"), &sig)?;
    // 恢复的 status 是旧 run 的；发布只要本轮真跑过的配方。done 同样
    // 无条件清：历史 partial 快照可能带着"跑完但带失败"的旧标记——
    // extras 没跑时它会被 save() 误读成完成（status 非空那条防线挡不
    // 住 status.txt 也恢复了旧内容的边角）/ a restored status file is
    // stale — publish wants only recipes that ran this time. done is
    // dropped unconditionally too: historical partial snapshots can carry
    // the old "finished with failures" marker — with extras never run it
    // could be misread as complete
    fsx::rm_rf(&e.prefix_win.join(".extras/status.txt"));
    fsx::rm_rf(&e.prefix_win.join(".extras/done"));
    shaderc_shadow(e);
    // 前缀快照 + 时间戳：发布差集与 dirty 检查的基线
    // prefix snapshot + stamp: the publish delta and dirty-check baseline
    fsx::write(
        &e.work.join("deps-before.txt"),
        &prefix_listing(e).join("\n"),
    )?;
    let _ = std::fs::File::create(e.work.join("deps-stamp"));
    Ok(())
}

/// 把 <lib.a> 里的符号 <old> 改名为 <new>：逐成员提取→nm 判定→objcopy
/// 改名→ar r 原位替换→ranlib（<tools> 为解析好的工具链；CLANGARM64 的
/// 整归档 objcopy 静默不干，必须逐成员）
/// Rename symbol <old> to <new> inside <lib.a>: extract each member ->
/// nm check -> objcopy rename -> `ar r` in place -> ranlib (<tools> is
/// the resolved toolchain; whole-archive objcopy is a silent no-op on
/// CLANGARM64, so member by member it is)
fn redefine_sym(lib: &Path, old: &str, new: &str, tools: &Toolchain) -> bool {
    let e = sys::env();
    let mdir = e.work.join(format!("rsym-{}", std::process::id()));
    fsx::rm_rf(&mdir);
    if fsx::install_d(&mdir).is_err() {
        return false;
    }
    // 绝不用 canonicalize：它返回 \\?\ verbatim 路径，ar 的 tempname
    // 往目录拼 "/stXXXXXX" 后 verbatim 语义不规范化斜杠 → 文件名成非法
    // 组件 → mkstemp EINVAL（"could not create temporary file whilst
    // writing archive"）。lib 本就是 prefix_win 下的绝对路径
    // Never canonicalize: it yields a \\?\ verbatim path — ar's tempname
    // appends "/stXXXXXX" to the dir, and verbatim paths never normalize
    // the slash -> an illegal "lib/stXXXXXX" component -> mkstemp EINVAL
    // ("could not create temporary file whilst writing archive"). lib is
    // already absolute under prefix_win
    let lib_s = lib.to_string_lossy().to_string();
    let members = sys::capture(&tools.ar, &["t", &lib_s], &e.work).unwrap_or_default();
    let mut hit = 0u32;
    for m in members.lines() {
        let m = m.trim();
        if m.is_empty() {
            continue;
        }
        // ar x 在 mdir 里解出成员文件（二进制安全——绝不用 ar p，它的
        // stdout 过文本模式会 \n→\r\n 毁掉对象）
        // ar x extracts the member inside mdir (binary-safe — never ar p,
        // whose stdout goes through text mode and inflates \n to \r\n)
        if sys::run(&tools.ar, &["x", &lib_s, m], &mdir).unwrap_or(1) != 0 {
            continue;
        }
        let mp = mdir.join(m);
        if !mp.is_file() {
            continue;
        }
        // [TtWwDdBbRr] old$ 或 U old$：定义或引用了该符号 / defines or
        // references the symbol
        let hits = sys::capture(&tools.nm, &[&mp.to_string_lossy()], &mdir)
            .unwrap_or_default()
            .lines()
            .any(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                (f.len() >= 3
                    && f[f.len() - 2].len() == 1
                    && "TtWwDdBbRr".contains(f[f.len() - 2])
                    && f[f.len() - 1] == old)
                    || (f.len() == 2 && f[0] == "U" && f[1] == old)
            });
        if hits {
            let ok = sys::run(
                &tools.objcopy,
                &[
                    "--redefine-sym",
                    &format!("{old}={new}"),
                    &mp.to_string_lossy(),
                ],
                &mdir,
            )
            .unwrap_or(1)
                == 0
                && sys::run(&tools.ar, &["r", &lib_s, &mp.to_string_lossy()], &mdir).unwrap_or(1)
                    == 0;
            if ok {
                hit += 1;
            }
        }
        let _ = std::fs::remove_file(&mp);
    }
    fsx::rm_rf(&mdir);
    if hit > 0 {
        let _ = sys::run(&tools.ranlib, &[&lib_s], &e.work);
        true
    } else {
        false
    }
}

/// 工具链名集合（nm/ar/ranlib/objcopy 经 PATH 解析）/ the resolved
/// toolchain names used by the archive surgeries
struct Toolchain {
    nm: String,
    ar: String,
    ranlib: String,
    objcopy: String,
}

/// 解析工具链：nm 缺席回退 llvm-nm；objcopy 依次试 env/llvm/objcopy
/// Resolve the tools: nm falls back to llvm-nm; objcopy tries env, llvm-,
/// plain in that order
fn tools(e: &Env) -> Toolchain {
    let nm = if sys::which(&e.nm).is_some() {
        e.nm.clone()
    } else {
        "llvm-nm".to_string()
    };
    let objcopy = ["llvm-objcopy", "objcopy"]
        .iter()
        .find(|c| sys::which(c).is_some())
        .map(|s| s.to_string())
        .unwrap_or_else(|| e.objdump.clone());
    Toolchain {
        nm,
        ar: e.ar.clone(),
        ranlib: e.ranlib.clone(),
        objcopy,
    }
}

/// 清旧 shim → pc 私有依赖/静态 cflag/系统库 →
/// 归档手术（dll.a 重写、sha1 改名、__imp_ 注入、rust personality、
/// blake2b 去重、kvazaar）→ librsvg 自检。在 overlay 恢复和 extras 之后
/// 跑——更早做会被它们写的文件覆盖。
/// The full deps_fixup port: purge stale shims -> pc private deps/static
/// cflags/system libs -> archive surgery (dll.a rewrite, sha1 rename,
/// __imp_ injection, rust personality, blake2b dedup, kvazaar) -> the
/// librsvg self-check. Runs after the overlay restore AND the extras —
/// any earlier and their writes race it.
pub fn fixup() -> Result<(), String> {
    let e = sys::env();
    let t = tools(e);
    let libdir = e.prefix_win.join("lib");
    let pcdir = libdir.join("pkgconfig");
    let sharepc = e.prefix_win.join("share/pkgconfig");

    // 先清残留 shim 成员：恢复来的前缀可能带着被污染的归档，GOMP 探测
    // 必须看到干净归档 / purge leftover shim members FIRST — a restored
    // prefix may carry poisoned archives and the GOMP probe must see them
    // pristine
    for la in fsx::walk(&libdir) {
        if la.extension().map(|x| x == "a").unwrap_or(false) {
            let m = crate::ar::members(&la, &e.work);
            if m.iter().any(|x| x == "shim.o" || x == "ffshim.o") {
                for s in ["shim.o", "ffshim.o"] {
                    let _ = sys::run(&t.ar, &["d", &la.to_string_lossy(), s], &e.work);
                }
                let _ = sys::run(&t.ranlib, &[&la.to_string_lossy()], &e.work);
            }
        }
    }

    log::group_begin("Patch pkg-config metadata");

    // ---- Requires.private 补齐（依赖真装了才加）/ Requires.private top-up
    // (added only when the dep is actually installed)
    // aribb24 的 DRCS 引用 png_* 却没声明 libpng / aribb24's DRCS code
    // references png_* without declaring libpng
    pkgcfg::require("aribb24", &["libpng"]);
    // chromaprint 链 FFT 后端（fftw3 双+单精度）/ chromaprint links an
    // FFT backend (fftw3 double+single)
    pkgcfg::require("libchromaprint", &["fftw3", "ffw3f"]);
    // FFmpeg 探测 "chromaprint" 而上游装 libchromaprint.pc——补别名
    // FFmpeg probes "chromaprint" while upstream ships libchromaprint.pc —
    // bridge the alias
    if !pkgcfg::pc_file("chromaprint").is_file() && pkgcfg::exists("libchromaprint") {
        let _ = fsx::write(
            &pcdir.join("chromaprint.pc"),
            &format!(
                "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: chromaprint\nDescription: chromaprint (pkg-config alias for libchromaprint.pc)\nVersion: {}\nRequires.private: libchromaprint\nLibs.private: {}\n",
                e.env_prefix,
                pkgcfg::modversion("libchromaprint"),
                e.cxxrt
            ),
        );
        log::log("created chromaprint.pc alias (libchromaprint)");
    }
    pkgcfg::require("libaribcaption", &["fontconfig", "freetype2", "harfbuzz"]);
    pkgcfg::require("fontconfig", &["expat", "freetype2", "iconv"]);
    pkgcfg::require("harfbuzz", &["freetype2", "graphite2", "glib-2.0"]);
    // freetype↔harfbuzz 互相循环：freetype2 必须在 -lfreetype 之后再发
    // -lharfbuzz / freetype<->harfbuzz are circular: freetype2 must re-emit
    // -lharfbuzz after -lfreetype for the archive order to resolve
    pkgcfg::require("freetype2", &["harfbuzz"]);
    // FFmpeg 探测 "libmp3lame" 而 MSYS2 只发 lame.pc——补 lame.pc 并架桥
    // FFmpeg probes "libmp3lame" while MSYS2 ships only lame.pc — patch
    // lame.pc and bridge the libmp3lame name
    pkgcfg::require("lame", &["libmpg123"]);
    if !pkgcfg::pc_file("libmp3lame").is_file()
        && pkgcfg::exists("lame")
        && pkgcfg::exists("libmpg123")
    {
        let _ = fsx::write(
            &pcdir.join("libmp3lame.pc"),
            &format!(
                "prefix={}\nexec_prefix=${{prefix}}\nlibdir=${{exec_prefix}}/lib\nincludedir=${{prefix}}/include\n\nName: libmp3lame\nDescription: LAME MP3 encoder (pkg-config alias for lame.pc)\nVersion: {}\nRequires.private: lame libmpg123\n",
                e.env_prefix,
                pkgcfg::modversion("lame")
            ),
        );
        log::log("created libmp3lame.pc alias (lame + libmpg123)");
    }

    // ---- Cflags.private 静态定义（没有它们头文件把 API 标成 dllimport）
    // Cflags.private static defines (without them the headers mark every
    // API dllimport)
    for (name, flags) in [
        ("glib-2.0", &["-DGLIB_STATIC_COMPILATION"][..]),
        ("gobject-2.0", &["-DGOBJECT_STATIC_COMPILATION"][..]),
        ("gio-2.0", &["-DGIO_STATIC_COMPILATION"][..]),
        ("cairo", &["-DCAIRO_WIN32_STATIC_BUILD"][..]),
        ("libchromaprint", &["-DCHROMAPRINT_NODLL"][..]),
        ("chromaprint", &["-DCHROMAPRINT_NODLL"][..]),
        ("libmodplug", &["-DMODPLUG_STATIC"][..]),
        ("libssh", &["-DLIBSSH_STATIC"][..]),
        ("twolame", &["-DLIBTWOLAME_STATIC"][..]),
        ("libzmq", &["-DZMQ_STATIC"][..]),
        (
            "libaribcaption",
            &["-DARIBCC_STATIC_DEFINE", "-DARIBCC_STATIC"][..],
        ),
        (
            "librabbitmq",
            &["-DLIBRABBITMQ_STATIC", "-DAMQP_STATIC"][..],
        ),
        ("openal", &["-DAL_LIBTYPE_STATIC"][..]),
    ] {
        pkgcfg::cflags_private(name, flags);
    }
    // openal.pc 的 -lOpenAL32 指 DLL；装的只有 libopenal.a
    // openal.pc's -lOpenAL32 points at the DLL; only libopenal.a exists
    let oapc = pcdir.join("openal.pc");
    if oapc.is_file() {
        let c = fsx::read(&oapc);
        if c.contains("-lOpenAL32") {
            let _ = std::fs::write(&oapc, c.replace("-lOpenAL32", "-lopenal"));
        }
    }
    // librabbitmq.pc 的游离 "-l"（"-l -lssl -lcrypto" → gcc 去找 lib-lssl）
    // librabbitmq.pc's stray "-l" ("-l -lssl -lcrypto" makes gcc look for
    // lib-lssl)
    let rmq = pcdir.join("librabbitmq.pc");
    if rmq.is_file() {
        let c = fsx::read(&rmq);
        let out = c.replace(" -l -l", " -l").replace("-l-l", "-l");
        if out != c {
            let _ = std::fs::write(&rmq, out);
        }
    }
    // libjxl_cms.a 的对象带 dllimport 过的 hwy 调用——真 hwy 归档必须在
    // 链接里 / libjxl_cms.a's objects carry dllimport'd hwy calls — the
    // real hwy archive must be in the link
    pkgcfg::require("libjxl", &["libhwy"]);

    // ---- Libs.private 系统库补齐 / Libs.private system-lib top-ups
    // 上游 .pc 忘写 C++ 运行时 / upstream .pc forgets the C++ runtime
    pkgcfg::libs_private("lensfun", &[&e.cxxrt]);
    pkgcfg::libs_private("libaribcaption", &[&e.cxxrt]);
    // DirectWrite 文本渲染器自己的系统库 / the DirectWrite renderer's own
    // system libs
    pkgcfg::libs_private(
        "libaribcaption",
        &["-ldwrite", "-ld2d1", "-lwindowscodecs", "-luuid", "-lole32"],
    );

    // OpenMP 运行时探测（提取成 ensure_gomp_probe：ffmpeg 配置阶段是
    // 独立进程，e.gomp 在 fixup 里的结果带不过进程边界，那边要自己
    // 再探一次）/ OpenMP runtime probe (extracted into
    // ensure_gomp_probe: the ffmpeg configure stage is a separate
    // process — the result probed here cannot cross the process
    // boundary and that side must probe for itself)
    ensure_gomp_probe();
    let gomp = e.gomp.read().unwrap().clone();
    // pacman .pc 里字面 -lgomp 在 clangarm64 上无物可满足——改写成探测
    // 到的运行时 / literal -lgomp in pacman .pc files is unsatisfiable on
    // clangarm64 — rewrite to the probed runtime
    if gomp != "-lgomp" {
        for pc in fsx::walk(&pcdir) {
            if pc.extension().map(|x| x == "pc").unwrap_or(false) {
                let c = fsx::read(&pc);
                if c.contains("-lgomp") {
                    let out: Vec<String> = c
                        .split(' ')
                        .map(|w| {
                            if w == "-lgomp" {
                                gomp.clone()
                            } else {
                                w.to_string()
                            }
                        })
                        .collect();
                    let _ = std::fs::write(&pc, out.join(" "));
                }
            }
        }
    }
    if !gomp.is_empty() {
        for name in ["soxr", "vidstab", "tesseract"] {
            pkgcfg::libs_private(name, &[&gomp]);
        }
    }
    // librist 的 PSK crypto 调 mbed*；-l: 钉住精确静态归档，避免 .dll.a
    // 给每个 exe 种 DLL 导入 / librist's PSK crypto calls mbed*; -l: pins
    // the exact static archive so no .dll.a plants DLL imports
    pkgcfg::libs_private(
        "librist",
        &["-l:libmbedtls.a", "-l:libmbedx509.a", "-l:libmbedcrypto.a"],
    );
    // libssh 的 TLS 后端 + socket/CryptoAPI/iphlpapi/pthread / libssh's TLS
    // backend plus socket/CryptoAPI/iphlpapi/pthread
    pkgcfg::libs_private(
        "libssh",
        &[
            "-lssl",
            "-lcrypto",
            "-lz",
            "-lws2_32",
            "-lcrypt32",
            "-lbcrypt",
            "-liphlpapi",
            "-lpthread",
        ],
    );
    // libzmq 的 socket 代码要 winsock / libzmq's socket code wants winsock
    pkgcfg::libs_private("libzmq", &["-lws2_32", "-lrpcrt4"]);
    // OpenAL win32 后端的 COM/mm API + avrt / OpenAL's win32 backend COM/mm
    // APIs plus avrt
    pkgcfg::libs_private("openal", &["-lole32", "-luuid", "-lwinmm", "-lavrt"]);
    // libplacebo 探测需基础 SPIRV-Tools 归档（opt 旗标字面量躺在它 .pc
    // 里的情形）/ libplacebo's probe needs the base SPIRV-Tools archive
    // (covers opt flags literal in its .pc)
    pkgcfg::libs_private("libplacebo", &["-lSPIRV-Tools"]);
    // tesseract 经 libarchive 读、libcurl 发帖——Requires 让两棵树都解析
    // tesseract reads via libarchive and posts through libcurl — Requires
    // resolves both trees
    pkgcfg::requires_private("tesseract", &["libcurl", "libarchive"]);
    // harfbuzz 循环：字面旗标不去重，落在 -lfreetype 之后才能解析回指
    // / the harfbuzz circle: a literal flag is not dedup'd and lands after
    // -lfreetype where the back-reference resolves
    pkgcfg::libs_private("freetype2", &["-lharfbuzz"]);
    // vendored 自建 glib 的上游 meson .pc 缺 MSYS2 打过补丁的系统库集合
    // / the vendored self-built glib's upstream meson .pc lacks MSYS2's
    // patched system-lib set
    pkgcfg::libs_private(
        "gio-2.0",
        &["-lintl", "-lshlwapi", "-ldnsapi", "-liphlpapi", "-lws2_32"],
    );
    // -latomic 探测：clangarm64 没有独立 libatomic.a（原子内建收进
    // compiler-rt），写死的 -latomic 让 lld 直接报 unable to find
    // library——-print-file-name 判真伪（gcc/clang 都支持）；解析不到
    // 就从 .pc 剥掉字面值并跳过 glib-2.0 的补录 / probe -latomic:
    // clangarm64 has no standalone libatomic.a (the atomic builtins live
    // inside compiler-rt) and a hardcoded -latomic makes lld fail with
    // "unable to find library" — -print-file-name decides (supported by
    // gcc and clang alike); when unresolvable, strip literal tokens
    // from .pc files and skip the glib-2.0 top-up
    let atomic = sys::capture(&e.cc, &["-print-file-name=libatomic.a"], &e.work)
        .map(|p| std::path::Path::new(p.trim()).is_file())
        .unwrap_or(false);
    if !atomic {
        for pc in fsx::walk(&pcdir) {
            if pc.extension().map(|x| x == "pc").unwrap_or(false) {
                let c = fsx::read(&pc);
                if c.contains("-latomic") {
                    let out: Vec<String> = c
                        .split(' ')
                        .filter(|w| *w != "-latomic")
                        .map(String::from)
                        .collect();
                    let _ = std::fs::write(&pc, out.join(" "));
                    log::log(&format!(
                        "patched {}: dropped -latomic (no libatomic.a in toolchain)",
                        pc.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
        }
    }
    if atomic {
        pkgcfg::libs_private("glib-2.0", &["-lintl", "-latomic", "-lm"]);
    } else {
        pkgcfg::libs_private("glib-2.0", &["-lintl", "-lm"]);
    }
    pkgcfg::libs_private("gobject-2.0", &["-lintl"]);
    pkgcfg::libs_private("gmodule-no-export-2.0", &["-lintl"]);
    // shaderc_combined 漏了自己对象要的 SPIRV-Tools + glslang 树（MSYS2
    // 不发 glslang.pc）/ shaderc_combined omits the SPIRV-Tools + glslang
    // tree its own objects need (MSYS2 ships no glslang.pc)
    for name in ["shaderc_combined", "shaderc"] {
        pkgcfg::libs_private(
            name,
            &[
                "-lSPIRV-Tools-opt",
                "-lSPIRV-Tools",
                "-lglslang",
                "-lSPIRV",
                "-lglslang-default-resource-limits",
            ],
        );
    }

    // ---- .pc 共享构建残留清理 / clean shared-build leftovers out of .pc
    //  * lib*.dll.a 字面路径（任意前缀写法）→ 静态兄弟在就改过去
    //  * -lgcc_s token → -static 下死重（解析出 libgcc_s.dll.a 导入）
    //  * literal lib*.dll.a paths (any prefix spelling) -> rewrite to the
    //    static sibling when it exists
    //  * -lgcc_s tokens -> dead weight under -static (they resolve to the
    //    libgcc_s.dll.a import lib)
    // deps 前缀 + env 前缀两棵 pkgconfig 树都扫：pacman 装的 .pc
    // 住在 env 前缀——旧补丁时代剥运算符的坏文件正是在那里随
    // env 缓存续命（gdk-pixbuf-2.0/cairo 的 Requires 裸版本对
    // 就是这么来的）/ scan BOTH pkgconfig trees — the deps prefix
    // AND the env prefix: pacman-installed .pc files live in the
    // env prefix, which is exactly where the sed-era operator
    // stripping damage hides and rides the env cache
    // (gdk-pixbuf-2.0/cairo bare `mod ver` Requires pairs)
    let envpc = [
        PathBuf::from(&e.env_prefix).join("lib/pkgconfig"),
        PathBuf::from(&e.env_prefix).join("share/pkgconfig"),
    ];
    let mut pcfiles: Vec<PathBuf> = Vec::new();
    for d in [&pcdir, &sharepc, &envpc[0], &envpc[1]] {
        for p in fsx::walk(d) {
            if p.extension().map(|x| x == "pc").unwrap_or(false) {
                pcfiles.push(p);
            }
        }
    }
    for pcf in &pcfiles {
        let c = fsx::read(pcf);
        let mut out = c.clone();
        // 先愈合 Requires 行被旧补丁剥掉的 >= 运算符——坏文件随
        // deps 快照/env 缓存每轮原样恢复，不每趟修就一直坏 /
        // heal the >= operators the older patcher stripped out of
        // Requires lines first — the damaged files are restored
        // verbatim from the deps snapshot/env cache every run, so
        // they stay broken unless re-healed every pass
        let healed = pkgcfg::heal_req_ops(&out);
        if healed != c {
            log::log(&format!(
                "patched {}: restored stripped Requires operators",
                pcf.file_name().unwrap().to_string_lossy()
            ));
            out = healed;
        }
        // 收集形如 lib*.dll.a 的 basename（任意路径前缀）/ collect
        // lib*.dll.a basenames under any prefix spelling
        let mut bases: Vec<String> = Vec::new();
        for w in c.split_whitespace() {
            let b = w.rsplit('/').next().unwrap_or(w);
            if b.starts_with("lib") && b.ends_with(".dll.a") {
                bases.push(b.to_string());
            }
        }
        bases.sort();
        bases.dedup();
        // 静态兄弟要到该 .pc 所属前缀的 lib 里找：<P>/lib|share/
        // pkgconfig/x.pc → <P>/lib（env 前缀的 pacman .pc 查 deps
        // libdir 既会漏判也会误判）/ resolve the static sibling in
        // the lib dir of THIS .pc's own prefix:
        // <P>/lib|share/pkgconfig/x.pc -> <P>/lib (looking up an
        // env-prefix pacman .pc in the deps libdir would both miss
        // and misfire)
        let own_lib = pcf.parent().and_then(|d| d.parent()).map(|d| {
            if d.ends_with("share") {
                d.parent().unwrap().join("lib")
            } else {
                d.to_path_buf()
            }
        });
        for b in bases {
            let sta = format!("{}.a", b.trim_end_matches(".dll.a"));
            if own_lib
                .as_ref()
                .map(|l| l.join(&sta).is_file())
                .unwrap_or(false)
            {
                out = out.replace(&b, &sta);
                log::log(&format!(
                    "patched {}: {b} -> {sta}",
                    pcf.file_name().unwrap().to_string_lossy()
                ));
            }
        }
        if out.contains("-lgcc_s") {
            out = out
                .split(' ')
                .filter(|w| *w != "-lgcc_s")
                .collect::<Vec<_>>()
                .join(" ");
            log::log(&format!(
                "patched {}: dropped -lgcc_s (libgcc_s_seh import lib)",
                pcf.file_name().unwrap().to_string_lossy()
            ));
        }
        if out != c {
            let _ = std::fs::write(pcf, out);
        }
    }

    // aribcc_config.h 烤死的 ARIBCC_SHARED_LIBRARY define——两种形态
    // （原始+旧 sed 破坏过的）都改写成干净注释 / the baked
    // ARIBCC_SHARED_LIBRARY define — both shapes (pristine and an older
    // sed's mangled one) rewrite to a clean comment
    let acfg = e.prefix_win.join("include/aribcaption/aribcc_config.h");
    if acfg.is_file() {
        let c = fsx::read(&acfg);
        let mut out = String::with_capacity(c.len());
        for l in c.lines() {
            if l.starts_with("#define ARIBCC_SHARED_LIBRARY") {
                out.push_str(&format!("/* {l} disabled for the static link */\n"));
            } else if l.starts_with("/* #define ARIBCC_SHARED_LIBRARY") {
                out.push_str("/* ARIBCC_SHARED_LIBRARY disabled for the static link */\n");
            } else {
                out.push_str(l);
                out.push('\n');
            }
        }
        if out != c {
            let _ = std::fs::write(&acfg, out);
            log::log("patched aribcc_config.h: dropped baked ARIBCC_SHARED_LIBRARY define");
        }
    }
    // libvvenc.pc 的 interface_libs-NOTFOUND 占位符 / libvvenc.pc's
    // interface_libs-NOTFOUND placeholder
    let vvenc = pcdir.join("libvvenc.pc");
    if vvenc.is_file() {
        let c = fsx::read(&vvenc);
        if c.contains("interface_libs-NOTFOUND") {
            let _ = std::fs::write(&vvenc, c.replace("interface_libs-NOTFOUND", ""));
        }
    }
    // whisper.cpp 的 ggml 归档没有 lib 前缀——复制成 lib<name>.a
    // whisper.cpp's ggml archives lack the lib prefix — copy to lib<name>.a
    for gg in fsx::walk(&libdir) {
        let name = gg.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with("ggml") && name.ends_with(".a") {
            let dst = libdir.join(format!("lib{name}"));
            if !dst.is_file() && std::fs::copy(&gg, &dst).is_ok() {
                log::log(&format!("renamed {name} -> lib{name} for -l resolution"));
            }
        }
    }

    // libzmq 的 external/sha1 与 libssh.a 的 md_crypto 撞通用名——改 zmq
    // 副本的名字，通用命名空间归 libssh / libzmq's external/sha1 collides
    // with libssh.a's md_crypto on the generic names — rename zmq's copies
    // and leave the generic namespace to libssh
    let zmq_a = libdir.join("libzmq.a");
    if zmq_a.is_file() && sys::which(&t.objcopy).is_some() {
        for s in ["sha1_init", "sha1_pad", "sha1_loop", "sha1_result"] {
            let _ = sys::run(
                &t.objcopy,
                &[
                    "--redefine-sym",
                    &format!("{s}=zmq_{s}"),
                    &zmq_a.to_string_lossy(),
                ],
                &e.work,
            );
        }
        let _ = sys::run(&t.ranlib, &[&zmq_a.to_string_lossy()], &e.work);
    }

    // ---- __imp_* shim 注入 / __imp_* shim injection
    // 对象里烤了 dllimport 调用（__imp_<sym> 引用）的归档用 cflag 修不了
    // ——对每个引用往拥有裸 <sym> 定义的归档注入 `void *__imp_<sym> =
    // &<sym>` 小成员。整归档一条 __imp_ 定义即标为导入库（跳过注入）。
    // Archives whose objects baked dllimport'd calls can't be fixed with
    // cflags — for each ref inject `void *__imp_<sym> = &<sym>` into the
    // archive owning the plain <sym> def. A single __imp_ definition marks
    // a whole archive as an import lib (never injected).
    {
        log::log("scanning archives for unresolved __imp_* references");
        let ars: Vec<PathBuf> = fsx::walk(&libdir)
            .into_iter()
            .filter(|p| p.extension().map(|x| x == "a").unwrap_or(false))
            .collect();
        // nm 扫描（多线程）→ need/def/implib 三集合 / parallel nm scan ->
        // the need/def/implib sets
        let chunks: Vec<Vec<PathBuf>> = {
            let n = e.jobs.max(1);
            let mut c: Vec<Vec<PathBuf>> = vec![Vec::new(); n];
            for (i, a) in ars.iter().enumerate() {
                c[i % n].push(a.clone());
            }
            c
        };
        // 每个归档线程的产物：(need 列表, (归档,符号) 定义对, 导入库集合,
        // 导入库拥有的 __imp_ 裸名集合)
        // per-archive thread yield: (needs, (archive,sym) def pairs,
        // import-lib set, plain names the import libs own)
        type ScanOut = (
            Vec<String>,
            Vec<(String, String)>,
            HashSet<String>,
            HashSet<String>,
        );
        let results: Vec<ScanOut> = std::thread::scope(|sc| {
            let handles: Vec<_> = chunks
                .into_iter()
                .map(|chunk| {
                    let nmt = t.nm.clone();
                    sc.spawn(move || {
                        let mut need = Vec::new();
                        let mut defs = Vec::new();
                        let mut implib = HashSet::new();
                        let mut implib_syms = HashSet::new();
                        for la in chunk {
                            let a = la.file_name().unwrap().to_string_lossy().to_string();
                            // current_dir 必须是目录——&la 是归档文件，
                            // spawn 会 ENOTDIR 被 unwrap_or_default 静默
                            // 吞掉（整个扫描空转，从不注入）
                            // current_dir must be a directory — &la is
                            // the archive FILE; the spawn fails ENOTDIR
                            // and unwrap_or_default swallows it silently
                            // (the whole scan goes dead, never injects)
                            let out = sys::capture(&nmt, &[&la.to_string_lossy()], &e.work)
                                .unwrap_or_default();
                            for l in out.lines() {
                                let f: Vec<&str> = l.split_whitespace().collect();
                                if f.len() == 2 && f[0] == "U" && f[1].starts_with("__imp_") {
                                    need.push(f[1][6..].to_string());
                                } else if f.len() >= 3 && f[2].starts_with("__imp_") {
                                    implib.insert(a.clone());
                                    implib_syms.insert(f[2][6..].to_string());
                                } else if f.len() >= 3
                                    && f[1].len() == 1
                                    && f[1].chars().next().unwrap().is_ascii_alphabetic()
                                {
                                    defs.push((a.clone(), f[2].to_string()));
                                }
                            }
                        }
                        (need, defs, implib, implib_syms)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut need: HashSet<String> = HashSet::new();
        let mut implib: HashSet<String> = HashSet::new();
        let mut implib_syms: HashSet<String> = HashSet::new();
        let mut prov: std::collections::HashMap<String, HashSet<String>> =
            std::collections::HashMap::new();
        for (n, d, i, is) in results {
            need.extend(n);
            implib.extend(i);
            implib_syms.extend(is);
            for (a, s) in d {
                prov.entry(a).or_default().insert(s);
            }
        }
        // 归约：拥有所需裸符号的归档成为提供者（导入库除外；导入库拥有
        // 的需求仍 shim——-static 下它不被拉入）/ reduction: an archive
        // owning a needed plain sym becomes the provider (except import
        // libs; implib-owned needs still get shims — under -static the
        // import lib is never pulled)
        let mut shimmed: Vec<PathBuf> = Vec::new();
        for (a, syms) in &prov {
            if a.ends_with(".dll.a") || implib.contains(a) {
                continue;
            }
            let wants: Vec<String> = syms.intersection(&need).cloned().collect::<Vec<_>>();
            if wants.is_empty() {
                continue;
            }
            let shimc = e.work.join("shim.c");
            let shimo = e.work.join("shim.o");
            // -fno-builtin -w：clang 把 printf/scanf/sprintf 家族认作内建
            // 函数，extern char <名>[] 声明成数组会撞 "redefinition as
            // different kind of symbol" 硬错——gcc 容忍、clangarm64 直接
            // 拒，整归档的 shim 连坐清空。关掉内建认知并压掉诊断噪音
            // / -fno-builtin -w: clang knows the printf/scanf/sprintf
            // family as builtins, so declaring extern char <name>[] hits
            // a hard "redefinition as different kind of symbol" error —
            // gcc tolerates it, clangarm64 refuses outright and the
            // whole archive's shims went down with it. Drop builtin
            // knowledge and hush the diagnostic noise
            let compile_shim = |body: &str| -> bool {
                let _ = fsx::write(&shimc, body);
                sys::run(
                    &e.cc,
                    &[
                        "-fno-builtin",
                        "-w",
                        "-c",
                        &shimc.to_string_lossy(),
                        "-o",
                        &shimo.to_string_lossy(),
                    ],
                    &e.work,
                )
                .unwrap_or(1)
                    == 0
            };
            let one = |s: &str| format!("extern char {s}[];\nvoid *__imp_{s} = (void*){s};\n");
            // 逐符号预检再编合集：单个编不过的名字不能让整归档的 shim
            // 陪葬 / preflight per-symbol then compile the survivors
            // together: one uncompilable name must not take the whole
            // archive's shims down with it
            let mut ok: Vec<String> = Vec::new();
            let mut dropped: Vec<String> = Vec::new();
            for s in &wants {
                if compile_shim(&one(s)) {
                    ok.push(s.clone());
                } else {
                    dropped.push(s.clone());
                }
            }
            if !dropped.is_empty() {
                log::warn(&format!(
                    "impshim: {a}: dropped {} uncompilable shim symbol(s): {}",
                    dropped.len(),
                    dropped.join(" ")
                ));
            }
            let prov_a = libdir.join(a);
            if !ok.is_empty()
                && compile_shim(&ok.iter().map(|s| one(s)).collect::<String>())
                && sys::run(
                    &t.ar,
                    &["rcs", &prov_a.to_string_lossy(), &shimo.to_string_lossy()],
                    &e.work,
                )
                .unwrap_or(1)
                    == 0
            {
                let _ = sys::run(&t.ranlib, &[&prov_a.to_string_lossy()], &e.work);
                shimmed.push(prov_a);
                log::log(&format!(
                    "injected {} __imp_* shim symbol(s) into {a}",
                    ok.len()
                ));
            }
            let _ = std::fs::remove_file(&shimc);
            let _ = std::fs::remove_file(&shimo);
        }
        // 保险闸：注入过的归档不能弄坏平凡链接；坏了全部撤回 / sanity
        // gate: injected archives must not break a trivial link — else
        // pull every shim back out
        if !shimmed.is_empty() {
            let tc = e.work.join("shimtest.c");
            let te = e.work.join("shimtest.exe");
            let _ = fsx::write(&tc, "int main(void){return 0;}\n");
            let ok = sys::run(
                &e.cc,
                &[
                    "-x",
                    "c",
                    &tc.to_string_lossy(),
                    "-static",
                    "-o",
                    &te.to_string_lossy(),
                ],
                &e.work,
            )
            .unwrap_or(1)
                == 0;
            let _ = std::fs::remove_file(&tc);
            let _ = std::fs::remove_file(&te);
            if !ok {
                log::warn(
                    "impshim: toolchain link test failed after injection — reverting all shims",
                );
                for la in &shimmed {
                    let _ = sys::run(&t.ar, &["d", &la.to_string_lossy(), "shim.o"], &e.work);
                    let _ = sys::run(&t.ranlib, &[&la.to_string_lossy()], &e.work);
                }
            }
        }
        // 覆盖核算：静态归档不提供的 need 还要看导入库收不收——kernel32/
        // msvcrt 这类 Win32 系统导入在 -static 下照常经工具链 .dll.a
        // 解析，本来就不该有静态提供者；只有两边都没有的才算真孤儿
        // / coverage accounting: a need with no static provider can still
        // be covered by an import lib — kernel32/msvcrt-class Win32
        // imports resolve through the toolchain's .dll.a under -static
        // as usual and were never meant to have a static provider; only
        // names owned by neither count as real orphans
        let mut provided: HashSet<String> = HashSet::new();
        for (a, syms) in &prov {
            if !a.ends_with(".dll.a") && !implib.contains(a) {
                provided.extend(syms.intersection(&need).cloned());
            }
        }
        let uncovered: HashSet<String> = need.difference(&provided).cloned().collect();
        let implib_covered = uncovered.intersection(&implib_syms).count();
        let orphans: Vec<String> = uncovered.difference(&implib_syms).cloned().collect();
        log::log(&format!(
            "impshim: {} need(s) — {} shimmed, {} import-lib covered, {} orphaned",
            need.len(),
            provided.len(),
            implib_covered,
            orphans.len()
        ));
        if !orphans.is_empty() {
            let mut o = orphans;
            o.sort();
            o.truncate(40);
            log::warn(&format!(
                "impshim: no provider for __imp_{} (first 40)",
                o.join(" __imp_")
            ));
        }
    }

    // librsvg vs librav1e 的 rust_eh_personality：双定义→改 rav1e 的；
    // rsvg 只引用→给弱 stub / rust_eh_personality: both define -> rename
    // rav1e's; rsvg-only-refs -> weak stub
    let rsv = libdir.join("librsvg-2.a");
    let rav = libdir.join("librav1e.a");
    if rsv.is_file() {
        let count = |p: &Path, kind: &str| -> usize {
            sys::capture(&t.nm, &[&p.to_string_lossy()], &e.work)
                .unwrap_or_default()
                .lines()
                .filter(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    match kind {
                        "def" => {
                            f.len() >= 3
                                && f[f.len() - 2].len() == 1
                                && "TtWw".contains(f[f.len() - 2])
                                && f[f.len() - 1] == "rust_eh_personality"
                        }
                        _ => f.len() == 2 && f[0] == "U" && f[1] == "rust_eh_personality",
                    }
                })
                .count()
        };
        let ravd = if rav.is_file() { count(&rav, "def") } else { 0 };
        let rsvd = count(&rsv, "def");
        let rsvu = count(&rsv, "undef");
        if ravd > 0 && rsvd > 0 {
            if redefine_sym(&rav, "rust_eh_personality", "rav1e_rust_eh_personality", &t) {
                log::log("renamed rust_eh_personality in librav1e.a (librsvg keeps its own)");
            } else {
                log::warn(
                    "rust_eh_personality rename failed on librav1e.a — final link may collide",
                );
            }
        } else if rsvd == 0 && rsvu > 0 {
            let c = e.work.join("ehp.c");
            let o = e.work.join("ehp.o");
            let _ = fsx::write(
                &c,
                "int __attribute__((weak)) rust_eh_personality(void){return 0;}\n",
            );
            if sys::run(
                &e.cc,
                &["-c", &c.to_string_lossy(), "-o", &o.to_string_lossy()],
                &e.work,
            )
            .unwrap_or(1)
                == 0
                && sys::run(
                    &t.ar,
                    &["rcs", &rsv.to_string_lossy(), &o.to_string_lossy()],
                    &e.work,
                )
                .unwrap_or(1)
                    == 0
            {
                log::log("added weak rust_eh_personality stub to librsvg-2.a");
            }
            let _ = std::fs::remove_file(&c);
            let _ = std::fs::remove_file(&o);
        } else {
            log::log(&format!(
                "rust_eh_personality: no action (rav1e-defs={ravd} rsvg-defs={rsvd})"
            ));
        }
    }

    // libb2 的公共 blake2b* 与 libsodium vendored 副本撞名——把交集中的
    // 每个符号在 libsodium 里改名 sodium_* / libb2's public blake2b*
    // collides with libsodium's vendored copy — rename every shared sym
    // inside libsodium to sodium_*
    let b2a = libdir.join("libb2.a");
    let soda = libdir.join("libsodium.a");
    if b2a.is_file() && soda.is_file() {
        let defined = |p: &Path| -> HashSet<String> {
            sys::capture(
                &t.nm,
                &["--defined-only", "-g", &p.to_string_lossy()],
                &e.work,
            )
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() >= 3 {
                    Some(f[2].to_string())
                } else {
                    None
                }
            })
            .collect()
        };
        let shared: Vec<String> = defined(&b2a)
            .intersection(&defined(&soda))
            .cloned()
            .collect();
        let mut rc = 0u32;
        for s in shared {
            if redefine_sym(&soda, &s, &format!("sodium_{s}"), &t) {
                rc += 1;
            } else {
                log::warn(&format!(
                    "could not rename '{s}' in libsodium.a — final link may collide"
                ));
            }
        }
        if rc > 0 {
            log::log(&format!(
                "renamed {rc} libb2-shared symbol(s) in libsodium.a (sodium_*)"
            ));
        }
    }

    // libxevd 与 libxeve 共享 mc/recon 源码——padding/prefetch_for_mc/
    // processDMVR 等同名非 static 符号同链必 multiple definition（实证：
    // x86_64 ffmpeg_g 链接全灭）。交集中的每个符号在 libxeve.a 里加
    // xeve_ 前缀改名——成员内引用一致换名，xeve_* API 不在交集不受影响；
    // linux 走 ld --allow-multiple-definition，mac 走
    // -multiply_defined,suppress / libxevd and libxeve share mc/recon
    // source — same-named non-static symbols (padding, prefetch_for_mc,
    // processDMVR, ...) are a guaranteed multiple definition in one
    // link (empirical: the x86_64 ffmpeg_g link died on them). Rename
    // every shared symbol inside libxeve.a with an xeve_ prefix —
    // member-internal references are renamed consistently, and the
    // xeve_* API is not in the intersection so callers stay intact;
    // linux rides ld --allow-multiple-definition and mac
    // -multiply_defined,suppress
    let xevd = libdir.join("libxevd.a");
    let xeve = libdir.join("libxeve.a");
    if xevd.is_file() && xeve.is_file() {
        let defined = |p: &Path| -> HashSet<String> {
            sys::capture(
                &t.nm,
                &["--defined-only", "-g", &p.to_string_lossy()],
                &e.work,
            )
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() >= 3 {
                    Some(f[2].to_string())
                } else {
                    None
                }
            })
            .collect()
        };
        let shared: Vec<String> = defined(&xevd)
            .intersection(&defined(&xeve))
            .cloned()
            .collect();
        let mut rc = 0u32;
        for s in shared {
            if redefine_sym(&xeve, &s, &format!("xeve_{s}"), &t) {
                rc += 1;
            } else {
                log::warn(&format!(
                    "could not rename '{s}' in libxeve.a — final link may collide"
                ));
            }
        }
        if rc > 0 {
            log::log(&format!(
                "renamed {rc} xevd-shared symbol(s) in libxeve.a (xeve_*)"
            ));
        }
    }

    // kvazaar.pc 缺上游配方自己加的静态链接位 / kvazaar.pc lacks the
    // static-linkage bits its own upstream recipe adds
    let kvz = pcdir.join("kvazaar.pc");
    if kvz.is_file() && !fsx::read(&kvz).contains("KVZ_STATIC_LIB") {
        let _ = fsx::append_line(&kvz, "Cflags.private: -DKVZ_STATIC_LIB");
        let _ = fsx::append_line(&kvz, "Libs.private: -lpthread");
        log::log("patched kvazaar.pc: KVZ_STATIC_LIB + -lpthread");
    }

    // librsvg 自检：重放 FFmpeg 的 -static pkg-config 链接探测，失败直接
    // 给出缺包/未解析符号 / librsvg self-check: replay FFmpeg's -static
    // pkg-config link probe so a failure names the missing package or the
    // unresolved symbols
    if pkgcfg::exists("librsvg-2.0") {
        if let Ok(plibs) = pkgcfg::libs_static("librsvg-2.0") {
            let c = e.work.join("rsvgprobe.c");
            let x = e.work.join("rsvgprobe.exe");
            let er = e.work.join("rsvgprobe.err");
            let _ = fsx::write(
                &c,
                "extern char rsvg_handle_new_from_data;\nint main(void){return (char*)&rsvg_handle_new_from_data!=0;}\n",
            );
            // gcc/clang 都认 @file 响应文件：--libs --static 把整棵依赖
            // 树摊成几百个 token，拼出的 argv 直接撞 Windows
            // CreateProcess 的 32767 字符上限——os error 206 "filename or
            // extension is too long"，进程还没 spawn 就死了。旗标落盘
            // 成响应文件后 argv 只剩一个 @路径
            // / both gcc and clang expand @file response files:
            // --libs --static flattens the whole dependency tree into
            // hundreds of tokens whose joined argv slams Windows'
            // 32767-char CreateProcess limit — os error 206 "filename or
            // extension is too long" kills the spawn before it starts.
            // Park the flags in a response file so argv is just @path
            let rsp = e.work.join("rsvgprobe.rsp");
            // 响应文件里 \ 是转义符——反斜杠路径要先翻倍；含空格的 token
            // 不支持（msys 前缀无空格）/ backslash is the escape char in
            // response files — double path separators first; space-bearing
            // tokens are unsupported (the msys prefix has none)
            let _ = fsx::write(&rsp, &plibs.replace('\\', "\\\\"));
            let args: Vec<String> = vec![
                "-static".into(),
                "-o".into(),
                x.to_string_lossy().to_string(),
                c.to_string_lossy().to_string(),
                format!("@{}", rsp.to_string_lossy()),
            ];
            let argrefs: Vec<&str> = args.iter().map(String::as_str).collect();
            // stderr 单独捕获给日志 / capture stderr for the log
            let mut cm = sys::cmd(&e.cc, &argrefs);
            cm.current_dir(&e.work);
            let res = cm.output();
            match res {
                Ok(o) if o.status.success() => {
                    log::log("librsvg self-check: pkg-config --static link resolves");
                }
                Ok(o) => {
                    log::warn("librsvg self-check FAILED — same error FFmpeg's probe will hit:");
                    let err = String::from_utf8_lossy(&o.stderr);
                    let _ = fsx::write(&er, &err);
                    for l in err.lines().take(25) {
                        log::log(&format!("    | {l}"));
                    }
                }
                Err(err) => log::warn(&format!("librsvg self-check spawn failed: {err}")),
            }
            for f in [&c, &x, &er, &rsp] {
                let _ = std::fs::remove_file(f);
            }
        }
    } else if sys::which("pkg-config").is_some() {
        log::warn("librsvg-2.0 fails pkg-config --exists:");
        // pkg-config 的诊断走 stderr 且包缺席即非零退出——sys::capture
        // 两头都踩（stderr 丢 null、非零直接 Err），明细从来打不出来；
        // 直接接管双流逐行落日志
        // pkg-config's diagnostics ride stderr and a missing package
        // means a non-zero exit — sys::capture loses on both counts
        // (stderr binned, non-zero -> Err), so the details never print.
        // Take over both streams and log them verbatim
        let mut cm = sys::cmd("pkg-config", &["--exists", "--print-errors", "librsvg-2.0"]);
        cm.current_dir(&e.work)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Ok(o) = cm.output() {
            let out = String::from_utf8_lossy(&o.stdout).replace('\r', "");
            let err = String::from_utf8_lossy(&o.stderr).replace('\r', "");
            for l in out.lines().chain(err.lines()).take(15) {
                log::log(&format!("    | {l}"));
            }
        }
        // 对 Requires 闭包逐成员点名——print-errors 含糊时这行直接给出
        // 哪个包缺席（私有成员用 --static 档探测）/ name each missing
        // member of the Requires closure outright — the answer survives a
        // vague --print-errors (private members probed at --static level)
        for (m, statik) in [
            ("glib-2.0", false),
            ("gio-2.0", false),
            ("gobject-2.0", false),
            ("gdk-pixbuf-2.0", false),
            ("cairo", false),
            ("gmodule-no-export-2.0", true),
            ("cairo-gobject", true),
            ("cairo-png", true),
            ("freetype2", true),
            ("harfbuzz", true),
            ("libxml-2.0", true),
            ("pangocairo", true),
            ("pangoft2", true),
        ] {
            let args: &[&str] = if statik {
                &["--exists", "--static", m]
            } else {
                &["--exists", m]
            };
            if sys::capture("pkg-config", args, &e.work).is_err() {
                log::log(&format!(
                    "    | missing Requires{} member: {m}",
                    if statik { ".private" } else { "" }
                ));
            }
        }
    }

    log::group_end();
    Ok(())
}

/// extras 阶段入口：恢复 deps overlay → 跑配方引擎（fixup 由引擎收尾
/// 的 postfix 工序内调，输出才进得了 extras/postfix.log）。
/// The extras stage entry: restore the deps overlay -> run the recipe
/// engine (fixup is invoked from inside the engine's trailing postfix
/// pass so its output lands in extras/postfix.log).
pub fn extras_stage() -> Result<(), String> {
    restore()?;
    crate::extras::run_all()
}

/// 差集清单 → 剔 pacman 拥有 → tar -19 → deps::publish。
/// complete = 配方表零失败走完（.extras/done + status 无 failed）；
/// dirty = 本轮有改动（status 行或文件差集/新时间戳）。
/// Delta manifest -> drop pacman-owned -> tar -19 ->
/// deps::publish. complete = the recipe table finished with zero failures
/// (.extras/done + no failed status lines); dirty = anything moved this
/// run (status lines or the file delta/newer-than-stamp set).
pub fn save() -> Result<(), String> {
    let e = sys::env();
    let platform = e.plat();
    let before = e.work.join("deps-before.txt");
    let stamp = e.work.join("deps-stamp");
    if !before.is_file() || !stamp.is_file() {
        log::warn(&format!(
            "no extras manifest at {:?}; nothing to publish",
            e.work
        ));
        return Ok(());
    }
    let sigv = fsx::read(&e.work.join("deps-sig"));
    if sigv.trim().is_empty() {
        log::warn("no deps signature recorded; skipping");
        return Ok(());
    }
    if sys::which("zstd").is_none() {
        log::warn("zstd missing; skipping deps publish");
        return Ok(());
    }
    let restored = fsx::read(&e.work.join("deps-restored")).trim().to_string();
    let statusf = e.prefix_win.join(".extras/status.txt");
    let status = fsx::read(&statusf);
    let complete = e.prefix_win.join(".extras/done").is_file()
        && !status.is_empty()
        && !status.lines().any(|l| l.ends_with(" failed"));
    let mut dirty = status
        .lines()
        .any(|l| l.ends_with(" ok") || l.ends_with(" failed"));
    if !deps::have_gh() {
        log::log("no GH_TOKEN/gh; deps cannot be published, skipping pack");
        return Ok(());
    }
    // 前缀差集兼当 dirty 检查 + 上传 manifest 源 / the prefix delta doubles
    // as the dirty check and feeds the upload manifest
    let after = prefix_listing(e);
    fsx::write(&e.work.join("deps-after.txt"), &after.join("\n"))?;
    let beforeset: HashSet<String> = fsx::read(&before).lines().map(|l| l.to_string()).collect();
    let added: Vec<String> = after
        .iter()
        .filter(|l| !beforeset.contains(*l))
        .cloned()
        .collect();
    fsx::write(&e.work.join("deps-added.txt"), &added.join("\n"))?;
    // 已存在但被替换的文件：时间戳晚于 deps-stamp / already-existing files
    // that were replaced: mtime newer than deps-stamp
    let stamp_mt = std::fs::metadata(&stamp)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let touched: Vec<String> = fsx::find_newer(&e.prefix_win, stamp_mt);
    fsx::write(&e.work.join("deps-touched.txt"), &touched.join("\n"))?;
    if !added.is_empty() || !touched.is_empty() {
        dirty = true;
    }
    // 反正不可能上传时跳过打包 / skip the tar when no upload can happen
    if !restored.is_empty() && restored != format!("deps-{platform}-partial.tar.zst") && !dirty {
        log::log("deps prefix unchanged since restored snapshot; skipping pack+upload");
        return Ok(());
    }
    if !complete && !dirty {
        log::log("deps unchanged and incomplete; skipping pack+upload");
        return Ok(());
    }
    let mut man: Vec<String> = Vec::new();
    man.extend(added);
    man.extend(touched);
    man.extend(
        fsx::read(&e.work.join("deps-old-manifest.txt"))
            .lines()
            .map(|l| l.to_string()),
    );
    // .deps-sig 在 stamp 之前写好，永不进差集——显式点名 / .deps-sig is
    // written before the stamp exists and never reaches the manifest via
    // the delta — name it explicitly
    man.push("./.deps-sig".to_string());
    man.sort();
    man.dedup();
    // 只留盘上实际存在的（旧 manifest 可能点名已消失文件）/ keep only
    // what is actually there (the old manifest may name vanished files)
    let man: Vec<String> = man
        .into_iter()
        .filter(|f| {
            let rel = f.trim_start_matches("./");
            e.prefix_win.join(rel.replace('/', "\\")).exists()
                || e.prefix_win.join(rel).exists()
                || std::fs::symlink_metadata(e.prefix_win.join(rel)).is_ok()
        })
        .collect();
    // 彻底丢掉 pacman 拥有的路径（重算——本轮 extras 可能写了新的
    // vendored 清单）/ drop pacman-owned paths for good (recompute — this
    // run's extras may have written new vendored manifests)
    let owned: HashSet<String> = pacman_files().into_iter().collect();
    let man: Vec<String> = man.into_iter().filter(|f| !owned.contains(f)).collect();
    fsx::write(&e.work.join("deps-manifest.txt"), &man.join("\n"))?;
    log::log(&format!(
        "publishing deps overlay, {} files (complete={complete} dirty={dirty})",
        man.len()
    ));
    let tarball = e.work.join("deps-upload.tar.zst");
    let tposix = sys::win_to_posix(&tarball, &e.msys2_root);
    let mposix = sys::win_to_posix(&e.work.join("deps-manifest.txt"), &e.msys2_root);
    sys::bash_ok(
        &format!(
            "tar -C '{}' -I 'zstd -19 -T0' -cf '{}' -T '{}'",
            e.env_prefix, tposix, mposix
        ),
        &e.work,
    )?;
    deps::publish(&platform, &restored, &tarball, complete, dirty)?;
    fsx::rm_rf(&tarball);
    Ok(())
}

/// OpenMP 运行时探测：真链接依次试 -lgomp、-lomp，结果落 e.gomp。
/// e.gomp 是进程内 RwLock——ffmpeg 配置阶段是独立进程，fixup 里探到
/// 的结果带不过进程边界，windows_common::ffmpeg 因此也调本函数。
/// CLANGARM64 上 pacman 依赖把 libomp.a 放进前缀，-lomp 兜底。
/// OpenMP runtime probe: a real link tries -lgomp then -lomp, result
/// lands in e.gomp. e.gomp is an in-process RwLock — the ffmpeg
/// configure stage is a separate process, the result probed inside
/// fixup cannot cross the process boundary, and
/// windows_common::ffmpeg therefore calls this too. On CLANGARM64 a
/// pacman dep lands libomp.a, hence the -lomp fallback.
pub fn ensure_gomp_probe() {
    let e = sys::env();
    let mut gomp = e.gomp.write().unwrap();
    if !gomp.is_empty() {
        return;
    }
    let tmp = e.work.join("omp-probe.c");
    let out = e.work.join("omp-probe.exe");
    let _ = fsx::write(&tmp, "int main(void){return 0;}\n");
    for cand in ["-lgomp", "-lomp"] {
        // capture 吃掉 lld "unable to find library" 噪音——探测失败本来
        // 就是预期分支，不该像真错误一样进日志 / capture swallows lld's
        // "unable to find library" noise — a probe miss is an expected
        // branch and should not land in the log looking like a real error
        if sys::capture(
            &e.cc,
            &[&tmp.to_string_lossy(), "-o", &out.to_string_lossy(), cand],
            &e.work,
        )
        .is_ok()
        {
            *gomp = cand.to_string();
            break;
        }
    }
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&out);
    log::log(&format!(
        "OpenMP runtime probe: {}",
        if gomp.is_empty() {
            "none"
        } else {
            gomp.as_str()
        }
    ));
}
