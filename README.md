# FFmpeg-Builds-Full

[简体中文](README.md) | [English](README.en.md)

全平台 FFmpeg 构建流水线：每 3 天自动构建上游 FFmpeg `master`（五平台同一提交，每平台四个变体——静态/共享/调试/共享+调试），经 GitHub Releases 发布。
全部为 **GPLv3、可再分发**构建（fdk-aac 等非自由库被刻意排除），且全部是**原生编译**——各平台在对应架构的 runner 上构建，不交叉编译、不用 Docker。

整条流水线由本仓库的 **Rust 驱动程序**（`FFmpeg-Builds-Full`，见根目录 `Cargo.toml`）执行：库配方、源码获取、FFmpeg configure 编排、deps 缓存与打包校验全部是 Rust 代码。

## 下载

每次构建产出**四个变体**：静态（默认，无后缀）、共享（`-shared`，可执行文件 + 共享库同目录旁位自解析）、调试（`-debug`，`-O0 -g3` 全符号）、共享+调试（`-shared-debug`，两形态叠加）。文件名：`ffmpeg-<上游提交前10位>-<平台>[-shared|-debug|-shared-debug].<扩展名>`，例如 `ffmpeg-46f23be8b1-windows-x86_64.7z`（静态）或 `ffmpeg-46f23be8b1-linux-x86_64-shared.tar.xz`。

**浮动 `latest`（链接固定，适合脚本）**：

```
https://github.com/ChanTrail/FFmpeg-Builds-Full/releases/latest/download/ffmpeg-latest-<platform>[-shared|-debug|-shared-debug].<ext>
```

`<platform>`：`windows-x86_64`、`windows-arm64`、`linux-x86_64`、`linux-arm64`、`macos-arm64`；Windows 为 `.7z`（固实压缩），其余为 `.tar.xz`。四个变体各有独立的 latest 链接。

**带日期的 Release**：标签 `YYYYMMDD`（同日多次构建附加 `-HHMM`），保留最近 30 个（约 3 个月）。每个压缩包附带：

- `<压缩包>.sha256`：`sha256sum -c` 校验（macOS：`shasum -a 256 -c`）；构建报告在包内 `BUILDINFO.info`——全部 configure 选项、启用/放弃的库

**预发布构建**：非 master 分支的驱动 CI 通过后自动触发 FFmpeg 构建，以 `<分支>-YYYYMMDD-HHMM` 标签发 pre-release，不影响 `latest`。

**macOS**：ad-hoc 签名、未经公证，解压后需执行 `xattr -dr com.apple.quarantine <解压目录>`。

## 平台与运行要求

| 平台 | 文件名后缀 | 构建环境 | 运行要求 |
|:--|:--|:--|:--|
| Windows x86_64 | `-windows-x86_64.7z` | `windows-latest`，MSYS2 UCRT64 | Windows 10+ |
| Windows arm64 | `-windows-arm64.7z` | `windows-11-arm`，MSYS2 CLANGARM64 | Windows 11 ARM64 |
| Linux x86_64 | `-linux-x86_64.tar.xz` | `ubuntu-26.04` | 任意 x86_64 Linux（全静态，无 glibc 要求） |
| Linux arm64 | `-linux-arm64.tar.xz` | `ubuntu-26.04-arm` | 任意 arm64 Linux（全静态，无 glibc 要求） |
| macOS arm64 | `-macos-arm64.tar.xz` | `macos-latest` | macOS 14+（Apple Silicon） |

产物静态程度：Windows 与 Linux 为**全静态**二进制（依赖库与运行库/glibc 全部编入）；macOS 为**依赖全静态 + 仅系统库动态**——Apple 不发行静态 libSystem，macOS 上不存在全静态可执行文件（VideoToolbox/AVFoundation 等系统框架也只能动态加载）。

### 平台特性

| | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:--|:--|:--|:--|
| TLS | GnuTLS（读系统证书库） | GnuTLS | GnuTLS | GnuTLS | Secure Transport（系统钥匙串） |
| 硬件解码 | D3D11VA / D3D12VA / DXVA2 / NVDEC / CUDA / QSV | D3D11VA / D3D12VA / DXVA2 / NVDEC / CUDA | VAAPI / VDPAU / NVDEC / CUDA / QSV / V4L2-M2M | VAAPI / VDPAU / NVDEC / CUDA / V4L2-M2M | VideoToolbox |
| 硬件编码 | NVENC / AMF / QSV / MediaFoundation | NVENC / MediaFoundation | NVENC / AMF / VAAPI / QSV | NVENC / VAAPI | VideoToolbox / AudioToolbox |
| 设备采集 | gdigrab / ddagrab / dshow | gdigrab / ddagrab / dshow | xcbgrab / V4L2 / ALSA / PulseAudio / KMS | xcbgrab / V4L2 / ALSA / PulseAudio / KMS | AVFoundation |
| GPU 滤镜 | Vulkan / OpenCL / libplacebo | Vulkan / OpenCL / libplacebo | Vulkan / OpenCL | Vulkan / OpenCL | CoreImage（Metal） |
| 音频插件宿主 | LV2 / LADSPA | LV2 / LADSPA | LV2 / LADSPA | LV2 / LADSPA | LV2 / LADSPA |

#### 各平台缺席项及原因

不是每个库都铺满五个平台——缺席的逐项列在这里，原因即 `features/`、`sources/` 清单注释的出处（"缺席于"未标架构 = 该 OS 全架构缺席）：

| 库/特性 | 缺席于 | 原因 |
|:--|:--|:--|
| AMF | Windows arm64、Linux arm64、macOS | AMD 专有 SDK 仅发行 x86_64——ARM 上无 AMD GPU 驱动栈可加载运行库；Linux ARM 的 AMD 加速由 VAAPI/Vulkan 承担 |
| QSV（libvpl） | Windows arm64、Linux arm64、macOS | Intel oneVPL 运行库仅 x86；ARM 平台没有 Intel GPU 驱动 |
| davs2 / xavs2 | Windows arm64、Linux arm64、macOS | 上游的 aarch64 汇编/内建代码编译不过，实际仅 x86_64 可用 |
| SVT-JPEG-XS | Windows arm64、Linux arm64、macOS | 上游实现绑定 x86 SIMD，非 x86 平台整库缺席 |
| lcevcdec / uavs3d | Windows arm64 | 上游提供 linux-arm64 汇编支持，但无 windows-arm64 移植 |
| libplacebo | Linux、macOS | 依赖整条 SPIRV-Cross + shaderc 工具链，当前只在 Windows 构建 |
| Vulkan / OpenCL | macOS | 前缀头文件/加载器 shim 配方仅覆盖 Windows+Linux；系统无 Vulkan loader，OpenCL.framework 已废弃未接入 |
| NVENC / NVDEC / CUVID / ffnvcodec | macOS | 无 NVIDIA GPU；头文件配方亦仅 Windows+Linux |
| VapourSynth | macOS | 上游仅产出落进 Python site-packages 的 .dylib——静态链接与运行时加载双无解 |
| GnuTLS | macOS | TLS 由 Secure Transport 系统钥匙串后端承担 |
| D3D11VA / D3D12VA / DXVA2 / MediaFoundation | Linux、macOS | Windows OS 接口 |
| VAAPI / VDPAU / V4L2-M2M / libdrm / ALSA / PulseAudio / xcb / libdc1394 | Windows、macOS | Linux 内核与系统 API |
| VideoToolbox / AudioToolbox / AVFoundation / CoreImage / Metal | Windows、Linux | macOS 系统框架 |

## 包含的库

下表按**构建输入**列出（`features/`、`sources/` 清单与 `src/recipes/` 配方）；某次构建的实际启用情况以随包 `BUILDINFO.info` 为准——个别库失败时只会缺席并记录在案，不会中断构建。

### 视频编解码

| 库 | 用途 | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| libx264 | H.264 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| x265 | HEVC 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| kvazaar | HEVC 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libaom | AV1 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| dav1d | AV1 解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| svt-av1 | AV1 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| rav1e | AV1 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvpx | VP8/VP9 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| openh264 | H.264 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| openapv | APV 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| xvid | MPEG-4 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libtheora | Theora 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| vvenc | VVC/H.266 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| xeve / xevd | EVC 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| lcevcdec | LCEVC 解码 | ✓ | — | ✓ | ✓ | ✓ |
| uavs3d | AVS3 解码 | ✓ | — | ✓ | ✓ | ✓ |
| davs2 / xavs2 | AVS2 解码/编码 | ✓ | — | ✓ | — | — |
| libjxl | JPEG XL | ✓ | ✓ | ✓ | ✓ | ✓ |
| libopenjpeg | JPEG 2000 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libwebp | WebP | ✓ | ✓ | ✓ | ✓ | ✓ |
| svtjpegxs | JPEG XS | ✓ | — | ✓ | — | — |
| AviSynth | 帧服务输入 | ✓ | ✓ | ✓ | ✓ | ✓ |

（缺席原因见上方"各平台缺席项及原因"表。）

### 音频编解码与处理

| 库 | 用途 | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| libopus | Opus | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvorbis | Vorbis | ✓ | ✓ | ✓ | ✓ | ✓ |
| libmp3lame | MP3 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| twolame | MP2 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libspeex | Speex | ✓ | ✓ | ✓ | ✓ | ✓ |
| libshine | MP3 定点编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libgsm | GSM | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcodec2 | Codec2 低码率语音 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libilbc | iLBC | ✓ | ✓ | ✓ | ✓ | ✓ |
| liblc3 | LC3 / LE Audio | ✓ | ✓ | ✓ | ✓ | ✓ |
| opencore-amrnb/wb | AMR 编解码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| vo-amrwbenc | AMR-WB 编码 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libsoxr | 高质量重采样 | ✓ | ✓ | ✓ | ✓ | ✓ |
| librubberband | 变速变调 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libbs2b | 耳机立体声增强 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libmysofa | HRTF 空间音频 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libmodplug / libopenmpt / libgme | 模块音乐 / 游戏音乐 | ✓ | ✓ | ✓ | ✓ | ✓ |
| chromaprint | 音频指纹 | ✓ | ✓ | ✓ | ✓ | ✓ |
| OpenAL | 3D 音频输出 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libflite | 语音合成 | ✓ | ✓ | ✓ | ✓ | ✓ |
| pocketsphinx | 语音识别 | ✓ | ✓ | ✓ | ✓ | ✓ |
| whisper.cpp | 语音转文字滤镜 ¹ | ✓ | ✓ | ✓ | ✓ | ✓ |
| ~~fdk-aac~~ | AAC 编解码 | 排除 | 排除 | 排除 | 排除 | 排除 |

¹ macOS 的 whisper.cpp 用 Metal GPU 后端（`af_whisper` 滤镜在 Apple Silicon 上走 GPU；Windows/Linux 为 Vulkan 后端，驱动经加载器按需载入，无硬性 GPU 依赖）。

### 字幕、字体、滤镜

| 库 | 用途 | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| libass | ASS/SSA 字幕渲染 | ✓ | ✓ | ✓ | ✓ | ✓ |
| freetype / harfbuzz / fribidi | 字体与整形 | ✓ | ✓ | ✓ | ✓ | ✓ |
| fontconfig / libunibreak | 字体配置 / 断行 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libaribcaption / aribb24 | ARIB 字幕（日本电视） | ✓ | ✓ | ✓ | ✓ | ✓ |
| libzvbi | 图文电视字幕 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libzimg | 色彩/缩放 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvidstab | 视频防抖 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libvmaf | 画质评估 | ✓ | ✓ | ✓ | ✓ | ✓ |
| frei0r | 视频特效插件 | ✓ | ✓ | ✓ | ✓ | ✓ |
| lcms2 | ICC 色彩管理 | ✓ | ✓ | ✓ | ✓ | ✓ |
| librsvg + cairo | SVG 解码 + `drawvg` 矢量绘制滤镜 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libplacebo + Vulkan | GPU 滤镜链 | ✓ | ✓ | — | — | — |
| OpenCL | GPU 滤镜 | ✓ | ✓ | ✓ | ✓ | — |
| lilv（LV2） | 音频插件宿主 | ✓ | ✓ | ✓ | ✓ | ✓ |
| LADSPA | 音频插件宿主 | ✓ | ✓ | ✓ | ✓ | ✓ |
| VapourSynth | 帧服务输入 | ✓ | ✓ | ✓ | ✓ | — |
| libtesseract | OCR 滤镜 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libqrencode / libquirc | 二维码生成/识别 | ✓ | ✓ | ✓ | ✓ | ✓ |
| liblensfun | 镜头校正 | ✓ | ✓ | ✓ | ✓ | ✓ |
| SDL2 | 输出设备 | ✓ | ✓ | ✓ | ✓ | ✓ |

### 协议、容器、光盘

| 库 | 用途 | Windows x86_64 | Windows arm64 | Linux x86_64 | Linux arm64 | macOS arm64 |
|:--|:--|:-:|:-:|:-:|:-:|:-:|
| GnuTLS | TLS（见上表） | ✓ | ✓ | ✓ | ✓ | — |
| Secure Transport | TLS（见上表） | — | — | — | — | ✓ |
| libsrt | SRT 协议 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libssh | SFTP 协议 | ✓ | ✓ | ✓ | ✓ | ✓ |
| librist | RIST 协议 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcurl | HTTP/HTTPS 等 | ✓ | ✓ | ✓ | ✓ | ✓ |
| zeromq | ZMQ 消息 | ✓ | ✓ | ✓ | ✓ | ✓ |
| librabbitmq | AMQP | ✓ | ✓ | ✓ | ✓ | ✓ |
| librtmp | RTMP（rtmpe/rtmps/rtmpt） | ✓ | ✓ | ✓ | ✓ | ✓ |
| libxml2 | DASH/XML | ✓ | ✓ | ✓ | ✓ | ✓ |
| libbluray / libudfread | 蓝光 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libdvdnav / dvdread / dvdcss | DVD | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcdio(-paranoia) | CD 抓取 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libcaca | 字符画输出 | ✓ | ✓ | ✓ | ✓ | ✓ |
| libdc1394 / raw1394 | IEEE1394 采集（内核 API，仅 Linux） | — | — | ✓ | ✓ | — |
| gmp | 大数运算 | ✓ | ✓ | ✓ | ✓ | ✓ |

### 压缩与系统

zlib、bzip2、xz（lzma）、snappy、iconv 编入 FFmpeg —— 五平台全有。zstd、brotli 仅为依赖栈成员（zstd 兼作 deps 包 `.tar.zst` 的压缩工具），未启用为 FFmpeg 特性（无 `--enable-libzstd`/`--enable-libbrotli`）。
Linux 另有：ALSA、PulseAudio、libxcb/X11 全家、V4L2、libdrm；以上均为静态链接。

> 支持性依赖（如 glib/cairo/pango、nettle/hogweed、SDL 的 X11 依赖等）随特性库自动带入，不单独列出。

## 构建方式

驱动程序是本仓库根目录的 Rust 单 crate（`cargo build --release` 产出一个无外部依赖的二进制），子命令即流水线各阶段：

```
FFmpeg-Builds-Full <build|build-all|packages|extras|ffmpeg|package|save-deps|smoke|features|sources>
```

`build-all` 一次建完全部产物变体（静态 → shared → debug → shared-debug）：共享底座（packages + extras）先跑一遍，随后逐变体出计划、四路并行 configure、变体间串行 `make -jN`、逐变体 install→package——依赖不重复构建，ffmpeg 本体编四遍；CI 的三平台构建步用的就是这条命令。

- **特性开关**（`features/*.yaml`）：FFmpeg configure 旗标的唯一真源。四层合并——`base.yaml`（永远启用）、`common.yaml`（全平台通用）、`misc.yaml`（按平台分节的杂项）、`<os>-<arch>.yaml`（平台特化）；键为特性名，值为 `enable`/`disable`，跨文件重复键直接报错。每个 `enable` 还要过本地探测（pkg-config / 文件存在），探不过自动降级 `disable`。
- **源码清单**（`sources/*.yaml`）：同构四层合并，值是获取方式——`archive` 固定产物 URL（可带 `sha256` 校验），`git` 仓库地址实时解析：默认分支 HEAD → GitHub/GitLab CI 验证 → 红/无 CI 回退最新版本 tag；`ref:` 可钉分支/tag/commit，`mirrors:` 给出备用仓（主仓克隆失败时按序回退）。下载走 reqwest 异步流式（断点续传+重试退避），构建与预取并发；git 拉取带停滞检测与递增退避重试，无子模块的源在 git 通道全灭时回退主机归档端点（GitHub `/archive/`、GitLab `/-/archive/`、googlesource `+archive` 等）。
- **库配方**（`src/recipes/`）：132 项配方，每库一个独立 Rust 模块，`common/`、`linux/`、`windows/` 按域分目录；每个 `.ok` 标记存配方源码哈希，内容变了自动重建。
- **依赖缓存**：依赖库编好后打成 `deps-<平台>-<哈希>.tar.zst` 存入固定 `deps` Release；之后每次运行还原（约 1–2 分钟）只编 FFmpeg 本体。配方/清单变更 → 新签名 → 自动全量重编；超时自动上传 `partial` 包下次续建。
- **平台包栈**（`packages/*.yaml`）：Windows 为 pacman 包表（UCRT64/CLANGARM64 双 profile），Linux 为 apt 包表（最新 GCC 降序探测 + pyenv 构建工具 + Rust/cargo-c），macOS 为 brew 包表。
- **确定性 + 严格的 configure**：五平台均 `--disable-autodetect`——只有清单显式点名的库参与构建，runner 镜像碰巧装上的东西不会悄悄混进二进制；点名库探测失败会被逐个剔除重试以枚举全部问题（各自 config.log 存档），但只要存在剔除就**拒绝构建**；必需库（x264、x265、aom、dav1d、vpx、opus、lame、freetype、libass、webp、zlib + 各平台 TLS）缺失立即报错。
- **四变体产物**：`build-all` 在每条平台腿内产出四份产物——静态（默认，`--disable-shared`，产物名无后缀）、shared（`--enable-shared --disable-static`，产物名 `-shared` 后缀）、debug（`--enable-debug=3 --disable-optimizations --disable-stripping`，`-O0 -g3` 全符号，产物名 `-debug` 后缀）、shared+debug（两开关叠加，产物名 `-shared-debug` 后缀）。共享底座（packages + extras）只跑一遍，ffmpeg 本体经"逐变体计划 → 四路并行 configure → 变体间串行 `make -jN` → 逐变体 install→package"编四遍，`dist/` 累积全部产物。依赖快照照常一条 `deps-<平台>` 通道（依赖库一律 `-fPIC` 编译，链进共享库天然合法）。
- **质量关口**：每平台腿的四份产物在干净 runner 上用驱动的 `smoke` 子命令逐个冒烟（真实编码 + 检查无 `--enable-nonfree` + 动态依赖白名单——shared 变体的旁位解析在此顺带实证）；五条腿全成功才发布，任一失败本次不发布。

### 驱动程序 CI（`ci.yml`）

- **所有分支 push / PR→master**：`fmt --check` + `cargo check` + `clippy -D warnings` + `cargo test`（三 OS 矩阵）。
- **master 上版本变化**：`Cargo.toml` 版本号对应 tag `Driver-<X.Y.Z>` 不存在 → 五平台构建驱动二进制 → 发该 tag 的正式 release。
- 其他分支只有测试，不构建也不发布。

## 触发与配置

- **CI→发布链**：`ci.yml` 在 master 全绿（测试 + 五平台驱动二进制 + FFmpeg 构建 + 干净机冒烟）后发 `repository_dispatch` 调起 `release.yml`——直接发布本次 CI 已验证的产物：驱动二进制发 `Driver-<版本>` tag（版本有变化时）+ 滑动 `Driver-latest` 桶，FFmpeg 产物发日期 tag release + 滑动 `latest`。产物只编一次，不重跑构建。
- **FFmpeg 定时构建**：cron `0 0 */3 * *`（每月 1、4、7…日 00:00 UTC / 北京时间 08:00；cron 无法精确表达"每 72 小时"，月末间隔会缩短为 1–2 天；整点任务高峰时 GitHub 可能延迟触发）。定时仅发生在 master（默认分支），且**首次完整成功构建之后才生效**（`latest` tag 存在为凭）。始终用已发布的最新驱动二进制去构建 FFmpeg，产物发正常日期 tag release 并滑动 `latest`。
- **非 master 分支**：不自动构建；可手动 dispatch `scheduled.yml`，产物以 **pre-release** 发布，不碰 `latest`。
- **手动**：Actions → "Scheduled FFmpeg Build" → "Run workflow"，可选 `publish` 是否发布。
- **驱动命令行选项**（`FFmpeg-Builds-Full <命令> [选项]`，全部支持 `--opt=value` 与 `--opt value` 两种写法）：`--work`/`--dist`/`--msys2`（路径）、`--repo`/`--ref`/`--sha`（钉源码——仓源与 ref 的事实源是 `sources/base.yaml` 的 `ffmpeg:` 键：主仓 `git://git.ffmpeg.org/ffmpeg.git` 失败按 `mirrors:` 顺序兜底，`--repo` 显式指定则单仓无回退）、`--mirror`（pacman 换源：ustc/tuna/aliyun/repo，短写 `-m`）、`--jobs`（并行数）、`--debug`（-g3 + 零优化 + 留符号）、`--shared`（共享构建，见下）、`--ccache`。每个选项透传成同名环境变量（`--work`→`FFBUILD_WORK` 等），CI workflow 直传 env 与命令行等效。
- **共享构建开关**：`--shared`（或 env `FFBUILD_SHARED` 非空）时产物翻转为共享形态（`--enable-shared --disable-static`）——`bin/` 内多出 `av*.dll`/`lib*.so.N`/`lib*.dylib` 并与可执行文件旁位自解析（Windows 靠 PE 目录搜索、macOS 靠 `@loader_path`、Linux 靠 patchelf 打的 `$ORIGIN`，均无需环境变量）；产物名带 `-shared` 后缀。依赖仍是静态库整体编入共享库，不额外发散 DLL/so。Linux/macOS 的依赖库一律 `-fPIC` 编译（x86_64/arm64 上代价≈0），同一份 deps 快照静态/共享通吃；macOS 产物依旧无法"全共享"——系统框架（VideoToolbox 等）永远是动态依赖。
- **纯环境变量**：`GITHUB_TOKEN`/`GH_TOKEN` 与 `GITLAB_TOKEN`/`GL_TOKEN`（git 源 CI 查询提额/私有实例，密钥走 env 不进命令行）、`HTTPS_PROXY` 等代理变量（reqwest 原生支持）；`FFBUILD_CHILD`/`FFMPEG_GDB` 为父子进程内部管道变量，非用户旋钮。

## 注意事项

- 仓库须保持**公开**——arm/macOS runner 仅对公开仓库免费。
- 首次运行（或清单/配方变更后首次）需全量编依赖，耗时数小时；单任务上限 6 小时，跑不完靠 `partial` 包续建。
- `deps` Release 由流水线自动管理，可整体删除，下次重建。
- 失败排查：对应 `logs-<平台>` artifact（保留 7 天）含各库日志；`build-<平台>` artifact（保留 3 天）含已完成的压缩包（四变体同包）。
- GitHub 会在公开仓库 60 天无活动后停用定时工作流；最稳妥的办法是偶尔提交。
- 源码实时解析依赖 GitHub/GitLab API；匿名调用限流低，CI 里 GitHub 自动用 `GITHUB_TOKEN` 提额，GitLab 侧可配置 secret/变量 `GITLAB_TOKEN`（含自托管实例）。本地网络不佳时挂代理（`HTTPS_PROXY`）。
- 需要 fdk-aac 等非自由库须自行构建，此类二进制**不可再分发**。


## 许可

构建产物为 **GPLv3**（FFmpeg 及各依赖库保留其自身许可证）。本仓库的驱动程序与流水线文件同样以 **GPLv3** 发布（见根目录 `LICENSE`）。
