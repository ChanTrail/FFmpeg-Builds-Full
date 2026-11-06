# FFmpeg-Builds-Full

[简体中文](README.md) | [English](README.en.md)

全平台 FFmpeg 静态构建流水线：每 3 天自动构建上游 FFmpeg `master` 的**静态二进制**（五平台同一提交），经 GitHub Releases 发布。
全部为 **GPLv3、可再分发**构建（fdk-aac 等非自由库被刻意排除），且全部是**原生编译**——各平台在对应架构的 runner 上构建，不交叉编译、不用 Docker。

整条流水线由本仓库的 **Rust 驱动程序**（`FFmpeg-Builds-Full`，见根目录 `Cargo.toml`）执行：库配方、源码获取、FFmpeg configure 编排、deps 缓存与打包校验全部是 Rust 代码。

## 下载

文件名：`ffmpeg-<上游提交前10位>-<平台>.<扩展名>`，例如 `ffmpeg-46f23be8b1-windows-x86_64.zip`。

**浮动 `latest`（链接固定，适合脚本）**：

```
https://github.com/ChanTrail/FFmpeg-Builds-Full/releases/latest/download/ffmpeg-latest-<platform>.<ext>
```

`<platform>`：`windows-x86_64`、`windows-arm64`、`linux-x86_64`、`linux-arm64`、`macos-arm64`；Windows 为 `.zip`，其余为 `.tar.xz`。

**带日期的 Release**：标签 `YYYYMMDD`（同日多次构建附加 `-HHMM`），保留最近 30 个（约 3 个月）。每个压缩包附带：

- `<压缩包>.sha256`：`sha256sum -c` 校验（macOS：`shasum -a 256 -c`）
- `<压缩包>.buildinfo.txt`：构建报告——全部 configure 选项、启用/放弃的库（压缩包内另有 `BUILDINFO.txt`）

**预发布构建**：非 master 分支的驱动 CI 通过后自动触发 FFmpeg 构建，以 `<分支>-YYYYMMDD-HHMM` 标签发 pre-release，不影响 `latest`。

**macOS**：ad-hoc 签名、未经公证，解压后需执行 `xattr -dr com.apple.quarantine <解压目录>`。

## 平台与运行要求

| 平台 | 文件名后缀 | 构建环境 | 运行要求 |
|:--|:--|:--|:--|
| Windows x86_64 | `-windows-x86_64.zip` | `windows-latest`，MSYS2 UCRT64 | Windows 10+ |
| Windows arm64 | `-windows-arm64.zip` | `windows-11-arm`，MSYS2 CLANGARM64 | Windows 11 ARM64 |
| Linux x86_64 | `-linux-x86_64.tar.xz` | `ubuntu-26.04` | 随 runner 的 glibc |
| Linux arm64 | `-linux-arm64.tar.xz` | `ubuntu-26.04-arm` | 随 runner 的 glibc |
| macOS arm64 | `-macos-arm64.tar.xz` | `macos-latest` | macOS 14+（Apple Silicon） |

### 平台特性

| | Windows | Linux | macOS |
|:--|:--|:--|:--|
| TLS | GnuTLS（读系统证书库） | GnuTLS | Secure Transport（系统钥匙串） |
| 硬件解码 | D3D11VA / D3D12VA / DXVA2 / NVDEC / CUDA / QSV* | VAAPI / VDPAU / NVDEC / CUDA / QSV* / V4L2-M2M | VideoToolbox |
| 硬件编码 | NVENC / AMF / QSV* / MediaFoundation | NVENC / AMF / VAAPI / QSV* | VideoToolbox / AudioToolbox |
| 设备采集 | gdigrab / ddagrab / dshow | xcbgrab / V4L2 / ALSA / PulseAudio / KMS | AVFoundation |
| GPU 滤镜 | Vulkan / OpenCL / libplacebo | Vulkan / OpenCL / libplacebo | CoreImage / OpenCL |
| 其他 | OpenGL 输出 | — | — |

\* 标注的 QSV（libvpl）仅 x86_64。

## 包含的库

下表按**构建输入**列出（`features/`、`sources/` 清单与 `src/recipes/` 配方）；某次构建的实际启用情况以随包 `BUILDINFO.txt` 为准——个别库失败时只会缺席并记录在案，不会中断构建。

### 视频编解码

| 库 | 用途 | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| libx264 | H.264 编码 | ✓ | ✓ | ✓ |
| x265 | HEVC 编码 | ✓ | ✓ | ✓ |
| kvazaar | HEVC 编码 | ✓ | ✓ | ✓ |
| libaom | AV1 编解码 | ✓ | ✓ | ✓ |
| dav1d | AV1 解码 | ✓ | ✓ | ✓ |
| svt-av1 | AV1 编码 | ✓ | ✓ | ✓ |
| rav1e | AV1 编码 | ✓ | ✓ | ✓ |
| libvpx | VP8/VP9 编解码 | ✓ | ✓ | ✓ |
| openh264 | H.264 编解码 | ✓ | ✓ | ✓ |
| openapv | APV 编解码 | ✓ | ✓ | ✓ |
| xvid | MPEG-4 编码 | ✓ | ✓ | ✓ |
| libtheora | Theora 编解码 | ✓ | ✓ | ✓ |
| vvenc | VVC/H.266 编码 | ✓ | ✓ | ✓ |
| xeve / xevd | EVC 编解码 | ✓ | ✓ | ✓ |
| lcevcdec | LCEVC 解码 | ✓ | ✓ | ✓ |
| uavs3d | AVS3 解码 | ✓ | ✓ | ✓ |
| davs2 / xavs2 ¹ | AVS2 解码/编码 | x64 | x64 | — |
| libjxl | JPEG XL | ✓ | ✓ | ✓ |
| libopenjpeg | JPEG 2000 | ✓ | ✓ | ✓ |
| libwebp | WebP | ✓ | ✓ | ✓ |
| svtjpegxs ¹ | JPEG XS | x64 | x64 | — |
| AviSynth | 帧服务输入 | ✓ | ✓ | ✓ |

¹ 上游没有可用的 aarch64 实现（davs2/xavs2 的 ARM 代码编译不过，SVT-JPEG-XS 是 x86-only），仅 x86_64。

### 音频编解码与处理

| 库 | 用途 | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| libopus | Opus | ✓ | ✓ | ✓ |
| libvorbis | Vorbis | ✓ | ✓ | ✓ |
| libmp3lame | MP3 编码 | ✓ | ✓ | ✓ |
| twolame | MP2 编码 | ✓ | ✓ | ✓ |
| libspeex | Speex | ✓ | ✓ | ✓ |
| libshine | MP3 定点编码 | ✓ | ✓ | ✓ |
| libgsm | GSM | ✓ | ✓ | ✓ |
| libcodec2 | Codec2 低码率语音 | ✓ | ✓ | ✓ |
| libilbc | iLBC | ✓ | ✓ | ✓ |
| liblc3 | LC3 / LE Audio | ✓ | ✓ | ✓ |
| opencore-amrnb/wb | AMR 编解码 | ✓ | ✓ | ✓ |
| vo-amrwbenc | AMR-WB 编码 | ✓ | ✓ | ✓ |
| libsoxr | 高质量重采样 | ✓ | ✓ | ✓ |
| librubberband | 变速变调 | ✓ | ✓ | ✓ |
| libbs2b | 耳机立体声增强 | ✓ | ✓ | ✓ |
| libmysofa | HRTF 空间音频 | ✓ | ✓ | ✓ |
| libmodplug / libopenmpt / libgme | 模块音乐 / 游戏音乐 | ✓ | ✓ | ✓ |
| chromaprint | 音频指纹 | ✓ | ✓ | ✓ |
| OpenAL | 3D 音频输出 | ✓ | ✓ | ✓ |
| libflite | 语音合成 | ✓ | ✓ | ✓ |
| pocketsphinx | 语音识别 | ✓ | ✓ | ✓ |
| whisper.cpp | 语音转文字滤镜 | ✓ | ✓ | ✓ ³ |
| ~~fdk-aac~~ | AAC 编解码 | 排除 | 排除 | 排除 |

### 字幕、字体、滤镜

| 库 | 用途 | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| libass | ASS/SSA 字幕渲染 | ✓ | ✓ | ✓ |
| freetype / harfbuzz / fribidi | 字体与整形 | ✓ | ✓ | ✓ |
| fontconfig / libunibreak | 字体配置 / 断行 | ✓ | ✓ | ✓ |
| libaribcaption / aribb24 | ARIB 字幕（日本电视） | ✓ | ✓ | ✓ |
| libzvbi | 图文电视字幕 | ✓ | ✓ | ✓ |
| libzimg | 色彩/缩放 | ✓ | ✓ | ✓ |
| libvidstab | 视频防抖 | ✓ | ✓ | ✓ |
| libvmaf | 画质评估 | ✓ | ✓ | ✓ |
| frei0r | 视频特效插件 | ✓ | ✓ | ✓ |
| lcms2 | ICC 色彩管理 | ✓ | ✓ | ✓ |
| librsvg | SVG 渲染 | ✓ | ✓ | ✓ |
| libplacebo + Vulkan | GPU 滤镜链 | ✓ | ✓ | ✓ ² |
| OpenCL | GPU 滤镜 | ✓ | ✓ | ✓ |
| lilv（LV2）/ LADSPA | 音频插件宿主 | ✓ | ✓ | ✓ |
| VapourSynth | 帧服务输入 | ✓ | ✓ | ✓ |
| libtesseract | OCR 滤镜 | ✓ | ✓ | ✓ |
| libqrencode / libquirc | 二维码生成/识别 | ✓ | ✓ | ✓ |
| liblensfun | 镜头校正 | ✓ | ✓ | ✓ |
| SDL2 | 输出设备 | ✓ | ✓ | ✓ |

² macOS 也编入了 `--enable-vulkan` / `--enable-libplacebo`，但系统没有 Vulkan loader——相关滤镜/硬编解码仅在安装 MoltenVK 等 loader 后可用。

³ macOS 的 whisper.cpp 用 Metal GPU 后端（`af_whisper` 滤镜在 Apple Silicon 上走 GPU；Windows/Linux 为 Vulkan 后端，驱动经加载器按需载入，无硬性 GPU 依赖）。

### 协议、容器、光盘

| 库 | 用途 | Win | Linux | macOS |
|:--|:--|:-:|:-:|:-:|
| GnuTLS / Secure Transport | TLS（见上表） | ✓ | ✓ | ✓ |
| libsrt | SRT 协议 | ✓ | ✓ | ✓ |
| libssh | SFTP 协议 | ✓ | ✓ | ✓ |
| librist | RIST 协议 | ✓ | ✓ | ✓ |
| libcurl | HTTP/HTTPS 等 | ✓ | ✓ | ✓ |
| zeromq | ZMQ 消息 | ✓ | ✓ | ✓ |
| librabbitmq | AMQP | ✓ | ✓ | ✓ |
| librtmp | RTMP（rtmpe/rtmps/rtmpt） | ✓ | ✓ | ✓ |
| libxml2 | DASH/XML | ✓ | ✓ | ✓ |
| libbluray / libudfread | 蓝光 | ✓ | ✓ | ✓ |
| libdvdnav / dvdread / dvdcss | DVD | ✓ | ✓ | ✓ |
| libcdio(-paranoia) | CD 抓取 | ✓ | ✓ | ✓ |
| libcaca | 字符画输出 | ✓ | ✓ | ✓ |
| libdc1394 / raw1394 | IEEE1394 采集（内核 API，仅 Linux） | — | ✓ | — |
| gmp | 大数运算 | ✓ | ✓ | ✓ |

### 压缩与系统

zlib、bzip2、xz（lzma）、zstd、brotli、snappy、iconv —— 三平台全有。
Linux 另有：ALSA、PulseAudio、libxcb/X11 全家、V4L2、libdrm；以上均为静态链接。

> 支持性依赖（如 glib/cairo/pango、nettle/hogweed、SDL 的 X11 依赖等）随特性库自动带入，不单独列出。

## 构建方式

驱动程序是本仓库根目录的 Rust 单 crate（`cargo build --release` 产出一个无外部依赖的二进制），子命令即流水线各阶段：

```
FFmpeg-Builds-Full <build|packages|extras|ffmpeg|package|save-deps|smoke|features|sources>
```

- **特性开关**（`features/*.yaml`）：FFmpeg configure 旗标的唯一真源。四层合并——`base.yaml`（永远启用）、`common.yaml`（全平台通用）、`misc.yaml`（按平台分节的杂项）、`<os>-<arch>.yaml`（平台特化）；键为特性名，值为 `enable`/`disable`，跨文件重复键直接报错。每个 `enable` 还要过本地探测（pkg-config / 文件存在），探不过自动降级 `disable`。
- **源码清单**（`sources/*.yaml`）：同构四层合并，值是获取方式——`archive` 固定产物 URL（可带 `sha256` 校验），`git` 仓库地址实时解析：默认分支 HEAD → GitHub/GitLab CI 验证 → 红/无 CI 回退最新版本 tag；`ref:` 可钉分支/tag/commit。下载走 reqwest 异步流式（断点续传+重试退避），构建与预取并发。
- **库配方**（`src/recipes/`）：66 项配方，每库一个独立 Rust 模块，`common/`、`linux/`、`windows/` 按域分目录；每个 `.ok` 标记存配方源码哈希，内容变了自动重建。
- **依赖缓存**：依赖库编好后打成 `deps-<平台>-<哈希>.tar.zst` 存入固定 `deps` Release；之后每次运行还原（约 1–2 分钟）只编 FFmpeg 本体。配方/清单变更 → 新签名 → 自动全量重编；超时自动上传 `partial` 包下次续建。
- **平台包栈**（`packages/*.txt`）：Windows 为 pacman 包表（UCRT64/CLANGARM64 双 profile），Linux 为 apt 包表（最新 GCC 降序探测 + pyenv 构建工具 + Rust/cargo-c），macOS 为 brew 包表。
- **确定性 + 严格的 configure**：三平台均 `--disable-autodetect`——只有清单显式点名的库参与构建，runner 镜像碰巧装上的东西不会悄悄混进二进制；点名库探测失败会被逐个剔除重试以枚举全部问题（各自 config.log 存档），但只要存在剔除就**拒绝构建**；必需库（x264、x265、aom、dav1d、vpx、opus、lame、freetype、libass、webp、zlib + 各平台 TLS）缺失立即报错。
- **质量关口**：每平台构建后在干净 runner 上用驱动的 `smoke` 子命令做冒烟测试（真实编码 + 检查无 `--enable-nonfree` + 动态依赖白名单）；五个平台全成功才发布，任一失败本次不发布。

### 驱动程序 CI（`ci.yml`）

- **所有分支 push / PR→master**：`fmt --check` + `cargo check` + `clippy -D warnings` + `cargo test`（三 OS 矩阵）。
- **master 上版本变化**：`Cargo.toml` 版本号对应 tag `Driver-<X.Y.Z>` 不存在 → 五平台构建驱动二进制 → 发该 tag 的正式 release。
- 其他分支只有测试，不构建也不发布。

## 触发与配置

- **驱动发布链**：驱动 release（tag `Driver-<版本>`）成功发布后自动链入一次 FFmpeg 构建（`repository_dispatch`）。构建用的始终是已发布的最新驱动二进制（浮动 `Driver-latest` 桶），不在仓内现编。
- **FFmpeg 定时构建**：cron `0 0 */3 * *`（每月 1、4、7…日 00:00 UTC / 北京时间 08:00；cron 无法精确表达"每 72 小时"，月末间隔会缩短为 1–2 天；整点任务高峰时 GitHub 可能延迟触发）。定时仅发生在 master（默认分支），且**首次完整成功构建之后才生效**（`latest` tag 存在为凭）。产物发正常日期 tag release 并滑动 `latest`。
- **非 master 分支**：该分支的驱动 CI 成功完成后自动发起 FFmpeg 构建（`workflow_run`），产物以 **pre-release** 发布，不碰 `latest`。
- **手动**：Actions → "Build FFmpeg" → "Run workflow"，可选 `publish` 是否发布。
- **驱动环境变量**（本地构建时用）：`FFBUILD_WORK`/`FFBUILD_DIST`/`FFBUILD_MSYS2`、`FFMPEG_REPO`/`FFMPEG_REF`/`FFMPEG_SHA`（钉源码）、`FFBUILD_MIRROR`（pacman 换源：ustc/tuna/aliyun/repo）、`GITHUB_TOKEN`/`GH_TOKEN` 与 `GITLAB_TOKEN`/`GL_TOKEN`（git 源 CI 查询提额/私有实例）、`HTTPS_PROXY` 等代理变量（reqwest 原生支持）。

## 注意事项

- 仓库须保持**公开**——arm/macOS runner 仅对公开仓库免费。
- 首次运行（或清单/配方变更后首次）需全量编依赖，耗时数小时；单任务上限 6 小时，跑不完靠 `partial` 包续建。
- `deps` Release 由流水线自动管理，可整体删除，下次重建。
- 失败排查：对应 `logs-<平台>` artifact（保留 7 天）含各库日志；`build-<平台>` artifact（保留 3 天）含已完成的压缩包。
- GitHub 会在公开仓库 60 天无活动后停用定时工作流；最稳妥的办法是偶尔提交。
- 源码实时解析依赖 GitHub/GitLab API；匿名调用限流低，CI 里 GitHub 自动用 `GITHUB_TOKEN` 提额，GitLab 侧可配置 secret/变量 `GITLAB_TOKEN`（含自托管实例）。本地网络不佳时挂代理（`HTTPS_PROXY`）。
- 需要 fdk-aac 等非自由库须自行构建，此类二进制**不可再分发**。


## 许可

构建产物为 **GPLv3**（FFmpeg 及各依赖库保留其自身许可证）。本仓库的驱动程序与流水线文件同样以 **GPLv3** 发布（见根目录 `LICENSE`）。
