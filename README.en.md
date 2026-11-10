# FFmpeg-Builds-Full

[简体中文](README.md) | [English](README.en.md)

A cross-platform FFmpeg build pipeline: every 3 days it builds upstream FFmpeg `master` (the same commit on all five platforms, two variants per platform — static/shared) and publishes them through GitHub Releases.
All builds are **GPLv3 and redistributable** (non-free libraries such as fdk-aac are deliberately excluded) and everything is **compiled natively** — each platform builds on a runner of its own OS and CPU architecture. No cross-compiling, no Docker.

The whole pipeline is driven by this repository's **Rust driver program** (`FFmpeg-Builds-Full`, see `Cargo.toml` at the root): library recipes, source fetching, FFmpeg's configure orchestration, the deps cache and packaging/verification are all Rust code.

## Downloads

Every build ships **two variants**: static (default, no suffix) and shared (`-shared`, executables plus shared libraries resolving sibling-wise in the same directory). File names: `ffmpeg-<first-10-of-upstream-commit>-<platform>[-shared].<ext>`, e.g. `ffmpeg-46f23be8b1-windows-x86_64.7z` (static) or `ffmpeg-46f23be8b1-linux-x86_64-shared.tar.xz`. Debug shapes are out of the full pipeline — build them manually via `--debug` / `--shared --debug` on single-variant runs when needed.

**Floating `latest` (stable links, script-friendly)**:

```
https://github.com/ChanTrail/FFmpeg-Builds-Full/releases/latest/download/ffmpeg-latest-<platform>[-shared].<ext>
```

`<platform>`: `windows-x86_64`, `windows-arm64`, `linux-x86_64`, `linux-arm64`, `macos-arm64`; Windows ships `.7z` (solid archive), the others `.tar.xz`. Both variants have their own latest links.

**Dated releases**: tagged `YYYYMMDD` (a same-day rebuild appends `-HHMM`), the newest 30 are kept (about 3 months). Every archive ships:

- `<archive>.sha256` — verify with `sha256sum -c` (macOS: `shasum -a 256 -c`); the build report lives inside the archive as `BUILDINFO.info` (every configure option plus the enabled/dropped libraries)

**Pre-release builds**: once a non-master branch's driver CI finishes successfully, an FFmpeg build of that branch starts automatically and ships as a pre-release tagged `<branch>-YYYYMMDD-HHMM` — `latest` is never touched.

**macOS**: ad-hoc signed, not notarized — run `xattr -dr com.apple.quarantine <extracted folder>` after downloading.

## Platforms & requirements

| Platform | File suffix | Build environment | Runtime requirement |
|:--|:--|:--|:--|
| Windows x86_64 | `-windows-x86_64.7z` | `windows-latest`, MSYS2 UCRT64 | Windows 10+ |
| Windows arm64 | `-windows-arm64.7z` | `windows-11-arm`, MSYS2 CLANGARM64 | Windows 11 ARM64 |
| Linux x86_64 | `-linux-x86_64.tar.xz` | `ubuntu-26.04` | any x86_64 Linux (fully static, no glibc requirement) |
| Linux arm64 | `-linux-arm64.tar.xz` | `ubuntu-26.04-arm` | any arm64 Linux (fully static, no glibc requirement) |
| macOS arm64 | `-macos-arm64.tar.xz` | `macos-latest` | macOS 14+ (Apple Silicon) |

How static the artifacts are: Windows and Linux ship **fully static** binaries (every dependency plus the runtime/glibc is linked in); macOS ships **all-static dependencies + system libraries dynamic** — Apple does not ship a static libSystem, so a fully static executable cannot exist on macOS (VideoToolbox/AVFoundation and friends are dynamic frameworks by nature).

### Platform features

| | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:--|:--|:--|:--|
| TLS | GnuTLS (system cert store) | GnuTLS | GnuTLS | GnuTLS | Secure Transport (system keychain) |
| HW decode | D3D11VA / D3D12VA / DXVA2 / NVDEC / CUDA / QSV | D3D11VA / D3D12VA / DXVA2 / NVDEC / CUDA | VAAPI / VDPAU / NVDEC / CUDA / QSV / V4L2-M2M | VAAPI / VDPAU / NVDEC / CUDA / V4L2-M2M | VideoToolbox |
| HW encode | NVENC / AMF / QSV / MediaFoundation | NVENC / MediaFoundation | NVENC / AMF / VAAPI / QSV | NVENC / VAAPI | VideoToolbox / AudioToolbox |
| Device capture | gdigrab / ddagrab / dshow | gdigrab / ddagrab / dshow | xcbgrab / V4L2 / ALSA / PulseAudio / KMS | xcbgrab / V4L2 / ALSA / PulseAudio / KMS | AVFoundation |
| GPU filters | Vulkan / OpenCL / libplacebo | Vulkan / OpenCL / libplacebo | Vulkan / OpenCL | Vulkan / OpenCL | CoreImage (Metal) |
| Audio plugin hosts | LV2 / LADSPA | LV2 / LADSPA | LV2 / LADSPA | LV2 / LADSPA | LV2 / LADSPA |

#### Per-architecture absences and why

Not every library lands on all five platforms — each absence is listed below; the reason is the same one noted in the `features/` and `sources/` manifests (an "Absent on" entry without an architecture means it is missing on every architecture of that OS):

| Library / feature | Absent on | Why |
|:--|:--|:--|
| AMF | Windows arm64, Linux arm64, macOS | AMD's proprietary SDK ships x86_64 only — no AMD GPU driver stack on ARM can load the runtime; AMD acceleration on Linux ARM is covered by VAAPI/Vulkan instead |
| QSV (libvpl) | Windows arm64, Linux arm64, macOS | Intel's oneVPL runtime is x86-only; ARM platforms have no Intel GPU driver |
| davs2 / xavs2 | Windows arm64, Linux arm64, macOS | Upstream's aarch64 assembly/intrinsics do not compile — effectively x86_64-only |
| SVT-JPEG-XS | Windows arm64, Linux arm64, macOS | Upstream implementation is bound to x86 SIMD — the whole library is absent on non-x86 |
| lcevcdec / uavs3d | Windows arm64 | Upstream ships linux-arm64 assembly support but no windows-arm64 port |
| libtesseract | Windows arm64 | The msys2 clangarm64 tesseract package lacks its NEON objects (DotProductNEON/intSimdMatrixNEON undefined — an upstream packaging defect) |
| libplacebo | Linux, macOS | Requires the whole SPIRV-Cross + shaderc toolchain — currently built on Windows only |
| Vulkan / OpenCL | macOS | The prefix header/loader shim recipes cover Windows+Linux only; the system has no Vulkan loader and the deprecated OpenCL.framework is not wired in |
| NVENC / NVDEC / CUVID / ffnvcodec | macOS | No NVIDIA GPU; the header recipes are also Windows+Linux only |
| VapourSynth | macOS | Upstream only produces a .dylib inside Python site-packages — a dead end both statically and at runtime |
| GnuTLS | macOS | TLS is served by the Secure Transport system-keychain backend |
| D3D11VA / D3D12VA / DXVA2 / MediaFoundation | Linux, macOS | Windows OS interfaces |
| VAAPI / VDPAU / V4L2-M2M / libdrm / ALSA / PulseAudio / xcb / libdc1394 | Windows, macOS | Linux kernel & system APIs |
| VideoToolbox / AudioToolbox / AVFoundation / CoreImage / Metal | Windows, Linux | macOS system frameworks |

## Included libraries

The tables list the **build inputs** (the `features/` and `sources/` manifests plus the `src/recipes/` recipes); what a given build actually enabled is recorded in its bundled `BUILDINFO.info` — a library that fails is simply absent and documented, it never aborts the build.

### Video codecs

| Library | Purpose | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| libx264 | H.264 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| x265 | HEVC encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| kvazaar | HEVC encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| libaom | AV1 codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| dav1d | AV1 decode | ✓ | ✓ | ✓ | ✓ | ✓ |
| svt-av1 | AV1 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| rav1e | AV1 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvpx | VP8/VP9 codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| openh264 | H.264 codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| openapv | APV codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| xvid | MPEG-4 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| libtheora | Theora codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| vvenc | VVC/H.266 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| xeve / xevd | EVC codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| lcevcdec | LCEVC decode | ✓ | — | ✓ | ✓ | ✓ |
| uavs3d | AVS3 decode | ✓ | — | ✓ | ✓ | ✓ |
| davs2 / xavs2 | AVS2 decode/encode | ✓ | — | ✓ | — | — |
| libjxl | JPEG XL | ✓ | ✓ | ✓ | ✓ | ✓ |
| libopenjpeg | JPEG 2000 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libwebp | WebP | ✓ | ✓ | ✓ | ✓ | ✓ |
| svtjpegxs | JPEG XS | ✓ | — | ✓ | — | — |
| AviSynth | frameserver input | ✓ | ✓ | ✓ | ✓ | ✓ |

(For why a cell is empty see the "Per-architecture absences" table above.)

### Audio codecs & processing

| Library | Purpose | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| libopus | Opus | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvorbis | Vorbis | ✓ | ✓ | ✓ | ✓ | ✓ |
| libmp3lame | MP3 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| twolame | MP2 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| libspeex | Speex | ✓ | ✓ | ✓ | ✓ | ✓ |
| libshine | fixed-point MP3 encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| libgsm | GSM | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcodec2 | Codec2 low-bitrate speech | ✓ | ✓ | ✓ | ✓ | ✓ |
| libilbc | iLBC | ✓ | ✓ | ✓ | ✓ | ✓ |
| liblc3 | LC3 / LE Audio | ✓ | ✓ | ✓ | ✓ | ✓ |
| opencore-amrnb/wb | AMR codec | ✓ | ✓ | ✓ | ✓ | ✓ |
| vo-amrwbenc | AMR-WB encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| libsoxr | high-quality resampling | ✓ | ✓ | ✓ | ✓ | ✓ |
| librubberband | time-stretch / pitch-shift | ✓ | ✓ | ✓ | ✓ | ✓ |
| libbs2b | binaural headphone enhancement | ✓ | ✓ | ✓ | ✓ | ✓ |
| libmysofa | HRTF spatial audio | ✓ | ✓ | ✓ | ✓ | ✓ |
| libmodplug / libopenmpt / libgme | tracker / game music | ✓ | ✓ | ✓ | ✓ | ✓ |
| chromaprint | audio fingerprinting | ✓ | ✓ | ✓ | ✓ | ✓ |
| OpenAL | 3D audio output | ✓ | ✓ | ✓ | ✓ | ✓ |
| libflite | speech synthesis | ✓ | ✓ | ✓ | ✓ | ✓ |
| pocketsphinx | speech recognition | ✓ | ✓ | ✓ | ✓ | ✓ |
| whisper.cpp | speech-to-text filter ¹ | ✓ | ✓ | ✓ | ✓ | ✓ |
| ~~fdk-aac~~ | AAC codec | excluded | excluded | excluded | excluded | excluded |

¹ macOS builds whisper.cpp with the Metal GPU backend (the `af_whisper` filter runs on the GPU on Apple Silicon; Windows/Linux use the Vulkan backend — the driver is pulled in by the loader only when used, so there is no hard GPU dependency).

### Subtitles, fonts, filters

| Library | Purpose | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| libass | ASS/SSA subtitle rendering | ✓ | ✓ | ✓ | ✓ | ✓ |
| freetype / harfbuzz / fribidi | fonts & shaping | ✓ | ✓ | ✓ | ✓ | ✓ |
| fontconfig / libunibreak | font config / line breaking | ✓ | ✓ | ✓ | ✓ | ✓ |
| libaribcaption / aribb24 | ARIB subtitles (Japanese TV) | ✓ | ✓ | ✓ | ✓ | ✓ |
| libzvbi | teletext subtitles | ✓ | ✓ | ✓ | ✓ | ✓ |
| libzimg | color/scale | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvidstab | video stabilization | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvmaf | quality assessment | ✓ | ✓ | ✓ | ✓ | ✓ |
| frei0r | video effect plugins | ✓ | ✓ | ✓ | ✓ | ✓ |
| lcms2 | ICC color management | ✓ | ✓ | ✓ | ✓ | ✓ |
| librsvg + cairo | SVG decoding + `drawvg` vector-graphics filter | ✓ | ✓ | ✓ | ✓ | ✓ |
| libplacebo + Vulkan | GPU filter chain | ✓ | ✓ | — | — | — |
| OpenCL | GPU filters | ✓ | ✓ | ✓ | ✓ | — |
| lilv (LV2) | audio plugin host | ✓ | ✓ | ✓ | ✓ | ✓ |
| LADSPA | audio plugin host | ✓ | ✓ | ✓ | ✓ | ✓ |
| VapourSynth | frameserver input | ✓ | ✓ | ✓ | ✓ | — |
| libtesseract | OCR filter | ✓ | — | ✓ | ✓ | ✓ |
| libqrencode / libquirc | QR code generate/detect | ✓ | ✓ | ✓ | ✓ | ✓ |
| liblensfun | lens correction | ✓ | ✓ | ✓ | ✓ | ✓ |
| SDL2 | output device | ✓ | ✓ | ✓ | ✓ | ✓ |

### Protocols, containers, optical media

| Library | Purpose | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| GnuTLS | TLS (see table above) | ✓ | ✓ | ✓ | ✓ | — |
| Secure Transport | TLS (see table above) | — | — | — | — | ✓ |
| libsrt | SRT protocol | ✓ | ✓ | ✓ | ✓ | ✓ |
| libssh | SFTP protocol | ✓ | ✓ | ✓ | ✓ | ✓ |
| librist | RIST protocol | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcurl | HTTP/HTTPS and more | ✓ | ✓ | ✓ | ✓ | ✓ |
| zeromq | ZMQ messaging | ✓ | ✓ | ✓ | ✓ | ✓ |
| librabbitmq | AMQP | ✓ | ✓ | ✓ | ✓ | ✓ |
| librtmp | RTMP (rtmpe/rtmps/rtmpt) | ✓ | ✓ | ✓ | ✓ | ✓ |
| libxml2 | DASH/XML | ✓ | ✓ | ✓ | ✓ | ✓ |
| libbluray / libudfread | Blu-ray | ✓ | ✓ | ✓ | ✓ | ✓ |
| libdvdnav / dvdread / dvdcss | DVD | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcdio(-paranoia) | CD ripping | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcaca | ASCII-art output | ✓ | ✓ | ✓ | ✓ | ✓ |
| libdc1394 / raw1394 | IEEE1394 capture (kernel API, Linux only) | — | — | ✓ | ✓ | — |
| gmp | bignum arithmetic | ✓ | ✓ | ✓ | ✓ | ✓ |

### Compression & system

zlib, bzip2, xz (lzma), zstd, brotli, snappy, iconv — on all five platforms.
Linux additionally: ALSA, PulseAudio, the libxcb/X11 family, V4L2, libdrm; all statically linked.

> Supporting dependencies (e.g. glib/cairo/pango, nettle/hogweed, SDL's X11 deps) are pulled in automatically by the feature libraries and are not listed individually.

## How it builds

The driver is a single Rust crate at the repository root (`cargo build --release` produces one dependency-free binary); its subcommands are the pipeline stages:

```
FFmpeg-Builds-Full <build|build-all|packages|extras|ffmpeg|package|save-deps|smoke|features|sources>
```

`build-all` produces every shipped variant in one run (static + shared): the shared base (packages + extras) runs once, then per-variant plans, two-way parallel configure, jobserver-pooled compile, and per-variant install→package — dependencies are never rebuilt, ffmpeg itself compiles twice; the CI build steps call exactly this command.

- **Feature switches** (`features/*.yaml`): the single source of truth for FFmpeg configure flags. Four merge layers — `base.yaml` (always on), `common.yaml` (all platforms), `misc.yaml` (misc items in per-platform sections) and `<os>-<arch>.yaml` (platform specifics); keys are feature names, values `enable`/`disable`, and a repeated key across files is a hard error. Every `enable` must also pass a local probe (pkg-config / file existence) — a failed probe degrades it to `disable`.
- **Source manifests** (`sources/*.yaml`): the same layered merge; values describe how to fetch — `archive` for fixed tarballs (optional `sha256` check), `git` for repositories resolved live: default-branch HEAD → GitHub/GitLab CI verification → fall back to the newest version tag when red or missing; `ref:` pins a branch/tag/commit, `mirrors:` lists fallback repos tried in order when the primary clone fails. Downloads are async streaming via reqwest (resume + retry backoff) and run concurrently with compilation; git pulls carry stall detection and escalating-backoff retries, and submodule-free sources fall back to host archive endpoints (GitHub `/archive/`, GitLab `/-/archive/`, googlesource `+archive`, …) when every git channel dies.
- **Library recipes** (`src/recipes/`): 132 recipes, one independent Rust module per library, grouped under `common/`, `linux/` and `windows/`; every `.ok` marker stores its recipe's source hash so changed content rebuilds automatically.
- **Dependency cache**: built dependencies pack as `deps-<platform>-<hash>.tar.zst` into the fixed `deps` Release; later runs restore it (~1–2 min) and only compile FFmpeg itself. A recipe/manifest change → a new signature → an automatic full rebuild; on timeout a `partial` package is uploaded so the next run resumes.
- **Platform package stacks** (`packages/*.yaml`): pacman on Windows (UCRT64/CLANGARM64 profiles), apt on Linux (newest GCC probed descending + a pyenv for build tools + Rust/cargo-c), brew on macOS.
- **Deterministic + strict configure**: all five platforms pass `--disable-autodetect` — only explicitly named libraries take part; whatever the runner image happens to install cannot sneak into the binary. A named library that fails its probe is dropped one by one to enumerate every problem (each config.log is archived), but any drop at all **refuses the build** — an incomplete artifact is never shipped. Missing required libraries (x264, x265, aom, dav1d, vpx, opus, lame, freetype, libass, webp, zlib + the platform TLS) fail immediately.
- **Two-variant products**: `build-all` emits two artifacts per platform leg — static (default, `--disable-shared`, no name suffix) and shared (`--enable-shared --disable-static`, `-shared` suffix). Debug (`--enable-debug=3 --disable-optimizations --disable-stripping`, `-O0 -g3` full symbols) and shared+debug stay out of the full pipeline — build them manually via `--debug` on single-variant runs. The shared base (packages + extras) runs once; ffmpeg itself compiles twice through per-variant plans → two-way parallel configure → jobserver-pooled compile → per-variant install→package, and `dist/` accumulates both products. One `deps-<platform>` snapshot channel as before (deps always compile `-fPIC`, so linking into shared libs is legal by construction).
- **Quality gate**: every platform leg's two artifacts each get the driver's `smoke` subcommand on a clean runner (real encodes + a check that no `--enable-nonfree` slipped in + a dynamic-dependency allowlist — the shared variant's sibling resolution is proven here in passing); the release only happens when all five legs succeed — any failure means no publish this round.

### Driver CI (`ci.yml`)

- **Every branch push / PRs to master**: `fmt --check` + `cargo check` + `clippy -D warnings` + `cargo test` (a three-OS matrix).
- **A version change on master**: the tag `Driver-<X.Y.Z>` matching `Cargo.toml`'s version does not exist → build the driver binary on all five platforms → publish that tag as a normal release.
- Other branches get tests only — no build, no release.

## Triggers & configuration

- **CI→release chain**: once `ci.yml` goes fully green on master (tests + five-platform driver binaries + FFmpeg builds + clean-machine smoke), it fires a `repository_dispatch` into `release.yml` — which publishes the artifacts that very CI run already verified: driver binaries ship under `Driver-<version>` (when the version changed) plus the rolling `Driver-latest` bucket, and FFmpeg products ship as a date-tagged release plus rolling `latest`. Artifacts compile once — nothing is rebuilt.
- **Scheduled FFmpeg builds**: cron `0 0 */3 * *` (00:00 UTC / 08:00 Beijing on the 1st, 4th, 7th… of each month; cron cannot express "every 72 hours" exactly, so the gap shortens to 1–2 days around month end; GitHub may delay triggers during peak minutes). Schedules only ever run on master (the default branch) **and only after the first complete successful build** (proven by the `latest` tag). They always build FFmpeg with the newest *released* driver binary, publishing normal date-tagged releases while rolling `latest` forward.
- **Non-master branches**: nothing builds automatically; `scheduled.yml` can be dispatched manually and ships as a **pre-release** — `latest` untouched.
- **Manual**: Actions → "Scheduled FFmpeg Build" → "Run workflow", with an optional `publish` toggle.
- **Driver CLI options** (`FFmpeg-Builds-Full <command> [options]` — both `--opt=value` and `--opt value` work): `--work`/`--dist`/`--msys2` (paths), `--repo`/`--ref`/`--sha` (source pinning — the `ffmpeg:` key in `sources/base.yaml` is the repo/ref source of truth: the primary `git://git.ffmpeg.org/ffmpeg.git` falls back through `mirrors:` in order, an explicit `--repo` is a single repo with no fallback), `--mirror` (pacman mirror: ustc/tuna/aliyun/repo, short `-m`), `--jobs` (parallelism), `--debug` (-g3 + no optimizations + keep symbols), `--shared` (shared build, below), `--ccache`. Each option forwards to its namesake env var (`--work` → `FFBUILD_WORK`, …), so CI workflows passing env directly are equivalent.
- **Shared-build switch**: `--shared` (or a non-empty `FFBUILD_SHARED`) flips the artifact to shared form (`--enable-shared --disable-static`) — `bin/` gains `av*.dll`/`lib*.so.N`/`lib*.dylib` sitting next to the executables and resolving sibling-wise (PE directory search on Windows, `@loader_path` on macOS, a patchelf'd `$ORIGIN` on Linux — no environment variables needed); the artifact name carries a `-shared` suffix. Dependencies stay static archives baked into the shared libraries — no extra DLL/.so sprawl. Deps always compile `-fPIC` on Linux/macOS (~free on x86_64/arm64), so one deps snapshot serves both static and shared products; macOS artifacts still can't be "fully shared" — system frameworks (VideoToolbox etc.) remain dynamic dependencies forever.
- **Env-only variables**: `GITHUB_TOKEN`/`GH_TOKEN` and `GITLAB_TOKEN`/`GL_TOKEN` (git-source CI quota / private instances — secrets ride env, not argv), the `HTTPS_PROXY` family (reqwest honours them natively); `FFBUILD_CHILD`/`FFMPEG_GDB` are internal parent↔child plumbing, not user knobs.

## Notes

- The repository must stay **public** — arm/macOS runners are free only for public repos.
- The first run (or the first after a manifest/recipe change) must compile all dependencies and takes hours; a single job is capped at 6 hours, and what doesn't finish resumes via the `partial` package.
- The `deps` Release is managed by the pipeline and can be deleted wholesale; it gets rebuilt next run.
- Debugging failures: the `logs-<platform>` artifact (kept 7 days) contains per-library logs; the `build-<platform>` artifact (kept 3 days) contains any finished archives (both variants in one).
- GitHub disables scheduled workflows after 60 days of inactivity on public repos; an occasional commit is the most reliable fix.
- Live source resolution needs the GitHub/GitLab APIs; anonymous calls are rate-limited aggressively. CI uses `GITHUB_TOKEN` automatically for GitHub, and a `GITLAB_TOKEN` secret/variable can be configured for GitLab (self-hosted instances included). On poor networks, set a proxy (`HTTPS_PROXY`).
- Non-free libraries like fdk-aac must be built by yourself — such binaries are **not redistributable**.


## License

The build artifacts are **GPLv3** (FFmpeg and each dependency keep their own licenses). The driver program and pipeline files in this repository are likewise released under **GPLv3** (see `LICENSE` in the root).
