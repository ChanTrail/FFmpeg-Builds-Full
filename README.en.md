# FFmpeg-Builds-Full

[简体中文](README.md) | [English](README.en.md)

A cross-platform static FFmpeg build pipeline: every 3 days it builds **static binaries** of upstream FFmpeg `master` (the same commit on all five platforms) and publishes them through GitHub Releases.
All builds are **GPLv3 and redistributable** (non-free libraries such as fdk-aac are deliberately excluded) and everything is **compiled natively** — each platform builds on a runner of its own OS and CPU architecture. No cross-compiling, no Docker.

The whole pipeline is driven by this repository's **Rust driver program** (`FFmpeg-Builds-Full`, see `Cargo.toml` at the root): library recipes, source fetching, FFmpeg's configure orchestration, the deps cache and packaging/verification are all Rust code.

## Downloads

File names: `ffmpeg-<first-10-of-upstream-commit>-<platform>.<ext>`, e.g. `ffmpeg-46f23be8b1-windows-x86_64.7z`.

**Floating `latest` (stable links, script-friendly)**:

```
https://github.com/ChanTrail/FFmpeg-Builds-Full/releases/latest/download/ffmpeg-latest-<platform>.<ext>
```

`<platform>`: `windows-x86_64`, `windows-arm64`, `linux-x86_64`, `linux-arm64`, `macos-arm64`; Windows ships `.7z` (solid archive), the others `.tar.xz`.

**Dated releases**: tagged `YYYYMMDD` (a same-day rebuild appends `-HHMM`), the newest 30 are kept (about 3 months). Every archive ships:

- `<archive>.sha256` — verify with `sha256sum -c` (macOS: `shasum -a 256 -c`); the build report lives inside the archive as `BUILDINFO.txt` (every configure option plus the enabled/dropped libraries)

**Pre-release builds**: once a non-master branch's driver CI finishes successfully, an FFmpeg build of that branch starts automatically and ships as a pre-release tagged `<branch>-YYYYMMDD-HHMM` — `latest` is never touched.

**macOS**: ad-hoc signed, not notarized — run `xattr -dr com.apple.quarantine <extracted folder>` after downloading.

## Platforms & requirements

| Platform | File suffix | Build environment | Runtime requirement |
|:--|:--|:--|:--|
| Windows x86_64 | `-windows-x86_64.7z` | `windows-latest`, MSYS2 UCRT64 | Windows 10+ |
| Windows arm64 | `-windows-arm64.7z` | `windows-11-arm`, MSYS2 CLANGARM64 | Windows 11 ARM64 |
| Linux x86_64 | `-linux-x86_64.tar.xz` | `ubuntu-26.04` | the runner's glibc |
| Linux arm64 | `-linux-arm64.tar.xz` | `ubuntu-26.04-arm` | the runner's glibc |
| macOS arm64 | `-macos-arm64.tar.xz` | `macos-latest` | macOS 14+ (Apple Silicon) |

### Platform features

| | Windows | Linux | macOS |
|:--|:--|:--|:--|
| TLS | GnuTLS (system cert store) | GnuTLS | Secure Transport (system keychain) |
| HW decode | D3D11VA / D3D12VA / DXVA2 / NVDEC / CUDA / QSV* | VAAPI / VDPAU / NVDEC / CUDA / QSV* / V4L2-M2M | VideoToolbox |
| HW encode | NVENC / AMF / QSV* / MediaFoundation | NVENC / AMF / VAAPI / QSV* | VideoToolbox / AudioToolbox |
| Device capture | gdigrab / ddagrab / dshow | xcbgrab / V4L2 / ALSA / PulseAudio / KMS | AVFoundation |
| GPU filters | Vulkan / OpenCL / libplacebo | Vulkan / OpenCL / libplacebo | CoreImage / OpenCL |
| Other | OpenGL output | — | — |

\* QSV (libvpl) marked entries are x86_64 only.

## Included libraries

The tables list the **build inputs** (the `features/` and `sources/` manifests plus the `src/recipes/` recipes); what a given build actually enabled is recorded in its bundled `BUILDINFO.txt` — a library that fails is simply absent and documented, it never aborts the build.

### Video codecs

| Library | Purpose | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| libx264 | H.264 encode | ✓ | ✓ | ✓ |
| x265 | HEVC encode | ✓ | ✓ | ✓ |
| kvazaar | HEVC encode | ✓ | ✓ | ✓ |
| libaom | AV1 codec | ✓ | ✓ | ✓ |
| dav1d | AV1 decode | ✓ | ✓ | ✓ |
| svt-av1 | AV1 encode | ✓ | ✓ | ✓ |
| rav1e | AV1 encode | ✓ | ✓ | ✓ |
| libvpx | VP8/VP9 codec | ✓ | ✓ | ✓ |
| openh264 | H.264 codec | ✓ | ✓ | ✓ |
| openapv | APV codec | ✓ | ✓ | ✓ |
| xvid | MPEG-4 encode | ✓ | ✓ | ✓ |
| libtheora | Theora codec | ✓ | ✓ | ✓ |
| vvenc | VVC/H.266 encode | ✓ | ✓ | ✓ |
| xeve / xevd | EVC codec | ✓ | ✓ | ✓ |
| lcevcdec | LCEVC decode | ✓ | ✓ | ✓ |
| uavs3d | AVS3 decode | ✓ | ✓ | ✓ |
| davs2 / xavs2 ¹ | AVS2 decode/encode | x64 | x64 | — |
| libjxl | JPEG XL | ✓ | ✓ | ✓ |
| libopenjpeg | JPEG 2000 | ✓ | ✓ | ✓ |
| libwebp | WebP | ✓ | ✓ | ✓ |
| svtjpegxs ¹ | JPEG XS | x64 | x64 | — |
| AviSynth | frameserver input | ✓ | ✓ | ✓ |

¹ No usable aarch64 implementation upstream (the ARM code of davs2/xavs2 does not compile, SVT-JPEG-XS is x86-only) — x86_64 only.

### Audio codecs & processing

| Library | Purpose | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| libopus | Opus | ✓ | ✓ | ✓ |
| libvorbis | Vorbis | ✓ | ✓ | ✓ |
| libmp3lame | MP3 encode | ✓ | ✓ | ✓ |
| twolame | MP2 encode | ✓ | ✓ | ✓ |
| libspeex | Speex | ✓ | ✓ | ✓ |
| libshine | fixed-point MP3 encode | ✓ | ✓ | ✓ |
| libgsm | GSM | ✓ | ✓ | ✓ |
| libcodec2 | Codec2 low-bitrate speech | ✓ | ✓ | ✓ |
| libilbc | iLBC | ✓ | ✓ | ✓ |
| liblc3 | LC3 / LE Audio | ✓ | ✓ | ✓ |
| opencore-amrnb/wb | AMR codec | ✓ | ✓ | ✓ |
| vo-amrwbenc | AMR-WB encode | ✓ | ✓ | ✓ |
| libsoxr | high-quality resampling | ✓ | ✓ | ✓ |
| librubberband | time-stretch / pitch-shift | ✓ | ✓ | ✓ |
| libbs2b | binaural headphone enhancement | ✓ | ✓ | ✓ |
| libmysofa | HRTF spatial audio | ✓ | ✓ | ✓ |
| libmodplug / libopenmpt / libgme | tracker / game music | ✓ | ✓ | ✓ |
| chromaprint | audio fingerprinting | ✓ | ✓ | ✓ |
| OpenAL | 3D audio output | ✓ | ✓ | ✓ |
| libflite | speech synthesis | ✓ | ✓ | ✓ |
| pocketsphinx | speech recognition | ✓ | ✓ | ✓ |
| whisper.cpp | speech-to-text filter | ✓ | ✓ | ✓ ³ |
| ~~fdk-aac~~ | AAC codec | excluded | excluded | excluded |

### Subtitles, fonts, filters

| Library | Purpose | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| libass | ASS/SSA subtitle rendering | ✓ | ✓ | ✓ |
| freetype / harfbuzz / fribidi | fonts & shaping | ✓ | ✓ | ✓ |
| fontconfig / libunibreak | font config / line breaking | ✓ | ✓ | ✓ |
| libaribcaption / aribb24 | ARIB subtitles (Japanese TV) | ✓ | ✓ | ✓ |
| libzvbi | teletext subtitles | ✓ | ✓ | ✓ |
| libzimg | color/scale | ✓ | ✓ | ✓ |
| libvidstab | video stabilization | ✓ | ✓ | ✓ |
| libvmaf | quality assessment | ✓ | ✓ | ✓ |
| frei0r | video effect plugins | ✓ | ✓ | ✓ |
| lcms2 | ICC color management | ✓ | ✓ | ✓ |
| librsvg | SVG rendering | ✓ | ✓ | ✓ |
| libplacebo + Vulkan | GPU filter chain | ✓ | ✓ | ✓ ² |
| OpenCL | GPU filters | ✓ | ✓ | ✓ |
| lilv (LV2) / LADSPA | audio plugin hosts | ✓ | ✓ | ✓ |
| VapourSynth | frameserver input | ✓ | ✓ | ✓ |
| libtesseract | OCR filter | ✓ | ✓ | ✓ |
| libqrencode / libquirc | QR code generate/detect | ✓ | ✓ | ✓ |
| liblensfun | lens correction | ✓ | ✓ | ✓ |
| SDL2 | output device | ✓ | ✓ | ✓ |

² macOS is also configured with `--enable-vulkan` / `--enable-libplacebo`, but the system has no Vulkan loader — the related filters/hw codecs only work once a loader such as MoltenVK is installed.

³ macOS builds whisper.cpp with the Metal GPU backend (the `af_whisper` filter runs on the GPU on Apple Silicon; Windows/Linux use the Vulkan backend — the driver is pulled in by the loader only when used, so there is no hard GPU dependency).

### Protocols, containers, optical media

| Library | Purpose | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| GnuTLS / Secure Transport | TLS (see table above) | ✓ | ✓ | ✓ |
| libsrt | SRT protocol | ✓ | ✓ | ✓ |
| libssh | SFTP protocol | ✓ | ✓ | ✓ |
| librist | RIST protocol | ✓ | ✓ | ✓ |
| libcurl | HTTP/HTTPS and more | ✓ | ✓ | ✓ |
| zeromq | ZMQ messaging | ✓ | ✓ | ✓ |
| librabbitmq | AMQP | ✓ | ✓ | ✓ |
| librtmp | RTMP (rtmpe/rtmps/rtmpt) | ✓ | ✓ | ✓ |
| libxml2 | DASH/XML | ✓ | ✓ | ✓ |
| libbluray / libudfread | Blu-ray | ✓ | ✓ | ✓ |
| libdvdnav / dvdread / dvdcss | DVD | ✓ | ✓ | ✓ |
| libcdio(-paranoia) | CD ripping | ✓ | ✓ | ✓ |
| libcaca | ASCII-art output | ✓ | ✓ | ✓ |
| libdc1394 / raw1394 | IEEE1394 capture (kernel API, Linux only) | — | ✓ | — |
| gmp | bignum arithmetic | ✓ | ✓ | ✓ |

### Compression & system

zlib, bzip2, xz (lzma), zstd, brotli, snappy, iconv — on all three platforms.
Linux additionally: ALSA, PulseAudio, the libxcb/X11 family, V4L2, libdrm; all statically linked.

> Supporting dependencies (e.g. glib/cairo/pango, nettle/hogweed, SDL's X11 deps) are pulled in automatically by the feature libraries and are not listed individually.

## How it builds

The driver is a single Rust crate at the repository root (`cargo build --release` produces one dependency-free binary); its subcommands are the pipeline stages:

```
FFmpeg-Builds-Full <build|packages|extras|ffmpeg|package|save-deps|smoke|features|sources>
```

- **Feature switches** (`features/*.yaml`): the single source of truth for FFmpeg configure flags. Four merge layers — `base.yaml` (always on), `common.yaml` (all platforms), `misc.yaml` (misc items in per-platform sections) and `<os>-<arch>.yaml` (platform specifics); keys are feature names, values `enable`/`disable`, and a repeated key across files is a hard error. Every `enable` must also pass a local probe (pkg-config / file existence) — a failed probe degrades it to `disable`.
- **Source manifests** (`sources/*.yaml`): the same layered merge; values describe how to fetch — `archive` for fixed tarballs (optional `sha256` check), `git` for repositories resolved live: default-branch HEAD → GitHub/GitLab CI verification → fall back to the newest version tag when red or missing; `ref:` pins a branch/tag/commit. Downloads are async streaming via reqwest (resume + retry backoff) and run concurrently with compilation.
- **Library recipes** (`src/recipes/`): 68 recipes, one independent Rust module per library, grouped under `common/`, `linux/` and `windows/`; every `.ok` marker stores its recipe's source hash so changed content rebuilds automatically.
- **Dependency cache**: built dependencies pack as `deps-<platform>-<hash>.tar.zst` into the fixed `deps` Release; later runs restore it (~1–2 min) and only compile FFmpeg itself. A recipe/manifest change → a new signature → an automatic full rebuild; on timeout a `partial` package is uploaded so the next run resumes.
- **Platform package stacks** (`packages/*.txt`): pacman on Windows (UCRT64/CLANGARM64 profiles), apt on Linux (newest GCC probed descending + a pyenv for build tools + Rust/cargo-c), brew on macOS.
- **Deterministic + strict configure**: all three platforms pass `--disable-autodetect` — only explicitly named libraries take part; whatever the runner image happens to install cannot sneak into the binary. A named library that fails its probe is dropped one by one to enumerate every problem (each config.log is archived), but any drop at all **refuses the build** — an incomplete artifact is never shipped. Missing required libraries (x264, x265, aom, dav1d, vpx, opus, lame, freetype, libass, webp, zlib + the platform TLS) fail immediately.
- **Quality gate**: after each platform builds, the driver's `smoke` subcommand runs on a clean runner (real encodes + a check that no `--enable-nonfree` slipped in + a dynamic-dependency allowlist); the release only happens when all five platforms succeed — any failure means no publish this round.

### Driver CI (`ci.yml`)

- **Every branch push / PRs to master**: `fmt --check` + `cargo check` + `clippy -D warnings` + `cargo test` (a three-OS matrix).
- **A version change on master**: the tag `Driver-<X.Y.Z>` matching `Cargo.toml`'s version does not exist → build the driver binary on all five platforms → publish that tag as a normal release.
- Other branches get tests only — no build, no release.

## Triggers & configuration

- **Driver-release chain**: a published driver release (tag `Driver-<version>`) automatically chains into one FFmpeg build (`repository_dispatch`). Builds always run the newest *released* driver binary (the floating `Driver-latest` bucket) — nothing is compiled in-place.
- **Scheduled FFmpeg builds**: cron `0 0 */3 * *` (00:00 UTC / 08:00 Beijing on the 1st, 4th, 7th… of each month; cron cannot express "every 72 hours" exactly, so the gap shortens to 1–2 days around month end; GitHub may delay triggers during peak minutes). Schedules only ever run on master (the default branch) **and only after the first complete successful build** (proven by the `latest` tag). They publish normal date-tagged releases while rolling `latest` forward.
- **Non-master branches**: once that branch's driver CI finishes successfully, an FFmpeg build starts automatically (`workflow_run`) and ships as a **pre-release** — `latest` untouched.
- **Manual**: Actions → "Build FFmpeg" → "Run workflow", with an optional `publish` toggle.
- **Driver environment variables** (for local builds): `FFBUILD_WORK`/`FFBUILD_DIST`/`FFBUILD_MSYS2`, `FFMPEG_REPO`/`FFMPEG_REF`/`FFMPEG_SHA` (source pinning), `FFBUILD_MIRROR` (pacman mirror switch: ustc/tuna/aliyun/repo), `GITHUB_TOKEN`/`GH_TOKEN` and `GITLAB_TOKEN`/`GL_TOKEN` (higher git-source CI query quota / private instances), and the `HTTPS_PROXY` family (reqwest honours them natively).

## Notes

- The repository must stay **public** — arm/macOS runners are free only for public repos.
- The first run (or the first after a manifest/recipe change) must compile all dependencies and takes hours; a single job is capped at 6 hours, and what doesn't finish resumes via the `partial` package.
- The `deps` Release is managed by the pipeline and can be deleted wholesale; it gets rebuilt next run.
- Debugging failures: the `logs-<platform>` artifact (kept 7 days) contains per-library logs; the `build-<platform>` artifact (kept 3 days) contains any finished archives.
- GitHub disables scheduled workflows after 60 days of inactivity on public repos; an occasional commit is the most reliable fix.
- Live source resolution needs the GitHub/GitLab APIs; anonymous calls are rate-limited aggressively. CI uses `GITHUB_TOKEN` automatically for GitHub, and a `GITLAB_TOKEN` secret/variable can be configured for GitLab (self-hosted instances included). On poor networks, set a proxy (`HTTPS_PROXY`).
- Non-free libraries like fdk-aac must be built by yourself — such binaries are **not redistributable**.


## License

The build artifacts are **GPLv3** (FFmpeg and each dependency keep their own licenses). The driver program and pipeline files in this repository are likewise released under **GPLv3** (see `LICENSE` in the root).
