<div align="center">

<img src="assets/logo.svg" alt="Ramen Logo" width="112" />

# Ramen

**专为 Windows 打造的毫秒级原生全盘文件极速搜索工具**

在一个清爽克制的原生桌面窗口内完成 NTFS 全盘文件毫秒级检索、实时增量同步、分类筛选排序与一键定位打开，并通过全局快捷键随叫随到、常驻系统托盘。  
基于 **Rust + [NSH](https://github.com/longbridge/nsh) + [Bento](https://github.com/Summe-rsnow/Bento) `v1.2.0`** 构建，直接集成 **[goz](https://github.com/mustafaahci/goz) `v0.7.1`** 索引服务，纯原生渲染，无 JavaScript 运行时开销。

<p>
  <a href="https://github.com/Summe-rsnow/Ramen/releases"><img src="https://img.shields.io/badge/Version-v1.2.0-f97316?style=flat-square" alt="Version v1.2.0" /></a>
  <img src="https://img.shields.io/badge/Rust-2024-orange?style=flat-square&logo=rust" alt="Rust 2024" />
  <img src="https://img.shields.io/badge/Platform-Windows-0ea5e9?style=flat-square" alt="Platform Windows" />
  <img src="https://img.shields.io/badge/UI-NSH%20Native-6366f1?style=flat-square" alt="UI NSH Native" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-146c2e?style=flat-square" alt="License Apache-2.0" /></a>
</p>

<p>
  <a href="#-核心特性">核心特性</a> ·
  <a href="#-搜索语法与快捷键">搜索语法与快捷键</a> ·
  <a href="#-快速开始">快速开始</a> ·
  <a href="#-项目结构">项目结构</a> ·
  <a href="#-家族工具集">家族工具集</a> ·
  <a href="https://github.com/Summe-rsnow/Ramen/releases">下载 Release</a>
</p>

</div>

---

## ✨ 核心特性

- **⚡ `goz v0.7.1` 毫秒级原生全盘索引**
  - 后台由 Windows 系统服务 `gozd.exe` 直接读取 NTFS 主文件表（`$MFT`）与 USN 变更日志维持全盘内存索引；前端以普通用户权限运行，避免每次启动 UAC 弹窗并保持文件拖拽与打开体验。
  - 通过 Named Pipe `\\.\pipe\goz-v1` 长连接通信，严格校验管道所有者为 `SYSTEM` 或 `Administrators`（防非特权管道劫持）；后台工作线程配合 `35ms` 输入防抖、`32ms` 事件队列轮询与 `Watch` 实时变更推送。
  - 状态栏实时展示匹配条目数、总体积、耗时（ms）、`volumes_incomplete` 构建提示与 `metadata_pending` 元数据补全状态。
- **🛠️ 开箱即用的服务集成与运维**
  - Windows 安装包（`.exe`）已随包捆绑固定版本 `gozd.exe v0.7.1`，并在安装阶段自动注册并启动 `goz` Windows 系统服务，安装完成打开即可直接搜索。
  - 若使用便携压缩包（`.zip`）或服务被手动停止，首次启动会展示清晰的引导视图，自动定位随包 `gozd.exe` 并支持一键 UAC 提权安装、升级、启动或卸载 `goz` 服务；检测到服务端版本与 `v0.7.1` 不一致时自动提示升级，设置弹窗内置分卷状态与 `gozd` 内存占用监控。
- **🔍 高密度桌面搜索体验**
  - 顶部搜索工具栏集成 `Aa` 区分大小写开关、路径作用域胶囊（右键「仅在此目录搜索」+ 一键清除）与 `?` 语法速查弹窗。
  - 提供 8 个实时文件分类筛选芯片（`全部` / `文件夹` / `文档` / `图片` / `视频` / `音频` / `压缩包` / `代码`）与可点击排序表头（`名称` / `路径` / `大小` / `修改时间`）。
  - 基于 34pt `virtual_list` 虚拟列表与零行级信号实现流畅滚动，每行支持双击/回车打开、悬浮快捷操作（打开 / 定位文件夹 / 复制路径）与完整右键菜单。
- **🖥️ 全局快捷键与原生桌面体验**
  - 内置系统级全局召唤快捷键（默认 `Alt+Space`，支持开关、快捷预设、自定义按键与冲突实时状态反馈），随时一键唤起/隐藏搜索窗口并聚焦输入框。
  - 自动记忆并恢复窗口尺寸与屏幕位置（支持最小化到托盘与退出前同步落盘），顶栏采用家族统一右置外观切换器，支持关闭行为设置、Windows 系统托盘常驻及 **GitHub Releases** 新版本检测。

---

## ⌨️ 搜索语法与快捷键

### 搜索语法速查

Ramen 支持空格逻辑与、引号短语以及 `goz` 兼容语法，点击搜索框右侧 `?` 按钮可随时查看并一键填入示例：

| 语法示例 | 说明 |
|---|---|
| `*.rs` | 通配符匹配文件扩展名 |
| `folder:` | 仅匹配目录文件夹 |
| `ext:pdf;docx` | 同时匹配多个扩展名（分号分隔） |
| `size:>10mb` | 按文件体积范围过滤（支持 `kb` / `mb` / `gb`） |
| `dm:today` | 按修改日期筛选（`today` / `yesterday` / `thisweek` 等） |
| `regex:` | 启用正则表达式匹配 |
| `!exclude` | 排除包含指定片段的路径或名称 |
| `C:\path\` | 限定路径前缀或配合右键「仅在此目录搜索」 |

### 快捷键

| 快捷键 | 作用域 | 功能 |
|---|---|---|
| `Alt+Space` | 全局（可配置） | 显示 / 隐藏窗口并聚焦搜索框 |
| `Ctrl+F` / `Ctrl+L` | 窗口内 | 聚焦搜索输入框并全选当前关键词 |
| `Up` / `Down` | 结果列表 | 上下移动选中结果行并滚动跟随 |
| `PageUp` / `PageDown` | 结果列表 | 快速翻页浏览结果 |
| `Enter` | 结果列表 | 打开选中项（未选时打开首项） |
| `Ctrl+Enter` | 结果列表 | 在资源管理器中定位并选中文件（Reveal） |
| `Ctrl+C` | 结果列表 | 复制选中项完整路径（输入框未选中文本时生效） |
| `Ctrl+,` | 窗口内 | 打开「设置与服务」管理对话框 |
| `Esc` | 分层递进 | 依次关闭顶层弹窗 → 清空搜索词 → 清除选中行 → 清除目录范围 → 托盘隐藏（当设为托盘模式时） |

---

## 🚀 快速开始

### 方式一：下载预编译安装包

前往 [GitHub Releases](https://github.com/Summe-rsnow/Ramen/releases) 下载最新版本的 Windows 安装包（`.exe`）或便携压缩包（`.zip`），解压 / 安装即可使用（已随包打包固定版本 `gozd.exe v0.7.1`）。

### 方式二：从源码运行与构建

#### 环境要求

- [Rust](https://www.rust-lang.org/tools/install)（Edition 2024，MSRV `1.97`）
- [NSH CLI](https://nsh.dev/docs/installation)（`0.7.1+`）

#### 运行开发版

```bash
# 克隆仓库
git clone https://github.com/Summe-rsnow/Ramen.git
cd Ramen

# 可选：提前拉取并校验固定版本 gozd.exe (v0.7.1)
.\scripts\fetch-gozd.ps1 -OutDir target\debug

# 以本机后端运行（推荐，会完整加载 nsh.json 与 theme.json 主题配置）
nsh run
```

> **提示**：直接使用 `cargo run` 也可以启动，但不会读取 `nsh.json` 和 `theme.json`，建议始终使用 `nsh run` 以获得完整的原生窗口与主题效果。

#### 检查、测试与打包

```bash
# 代码、类型与构建检查
nsh check

# 运行单元测试、Headless UI 交互测试与 TypeScript 端到端测试
nsh test

# 打包 Windows 发行版产物（输出至 .nsh/out/）
nsh package --target windows
```

---

## 📁 项目结构

```text
Ramen/
├── .github/workflows/
│   └── release.yml         # Tag 推送自动构建、打包 gozd.exe 并发布 GitHub Release 工作流
├── assets/                 # 应用图标、Logo、品牌标与 64×64 托盘图标
├── scripts/
│   └── fetch-gozd.ps1      # goz v0.7.1 下载与 SHA-256 校验脚本
├── nsh.json                # 应用元信息、版本号（1.2.0）、窗口配置与主题声明
├── theme.json              # 浅色 / 深色语义色 Token（与 Bento 家族规范对齐）
├── DESIGN.md               # 设计规范、布局架构、动效清单与无障碍约定
├── src/
│   ├── lib.rs              # 根组件 App（#[native_module]）与 APP_META 常量
│   ├── main.rs             # 桌面二进制入口（1040×680，单实例）
│   ├── shell.rs            # 桌面壳：48px 隐藏标题栏、窗口记忆、全局快捷键、托盘与快捷键
│   ├── dialogs.rs          # 设置/快捷键/服务管理弹窗、卸载确认、语法速查与关于弹窗
│   ├── goz/                # goz v0.7.1 协议与 Windows 服务托管层
│   │   ├── mod.rs          # 模块导出与公共类型重导出
│   │   ├── version.rs      # gozd v0.7.1 固定版本、下载校验与二进制定位
│   │   ├── service.rs      # Windows SCM 状态查询与 UAC 提权安装/启动/卸载
│   │   ├── query.rs        # FileCategory 分类映射、路径切分与文件图标类型推断
│   │   └── client.rs       # Named Pipe 客户端、管道安全校验与 SearchEngine 异步引擎
│   ├── store/              # 私有 Signal 状态层（零公开 Signal）
│   │   ├── mod.rs          # 状态定义、ramen.prefs.v1 持久化、Test Seeding 与单元测试
│   │   └── actions.rs      # 35ms 搜索防抖、32ms 事件轮询、服务动作、打开定位与更新检测
│   ├── views/              # 业务工作区视图
│   │   ├── mod.rs          # 视图模块导出
│   │   ├── onboarding.rs   # 未安装 / 已停止 / 管道异常服务引导卡片
│   │   └── search.rs       # 主搜索区、分类过滤条、virtual_list 结果表与 28px 状态栏
│   └── ui/                 # 统一导出 bento::ui 与 Ramen 专属搜索控件
│       ├── mod.rs          # bento::ui::* / bento::format::* 重导出
│       ├── icons.rs        # 复用 bento::ui::icons，托管 Ramen 品牌标与托盘图标
│       └── components.rs   # 搜索栏、范围胶囊、可排序表头、零信号结果行与右键菜单
├── tests/
│   └── screen.rs           # Rust Headless 端到端 UI 交互测试
└── App.test.tsx            # TypeScript 端到端测试入口
```

---

## 🍱 家族工具集

| 工具 | 当前版本 | 仓库地址 | 定位与职责 |
|---|---|---|---|
| **Bento** | `v1.3.1` | [Summe-rsnow/Bento](https://github.com/Summe-rsnow/Bento) | 共享原生 UI 组件库、统一 24×24 图标库与桌面壳基础设施 |
| **Wonton** | `v1.4.0` | [Summe-rsnow/Wonton](https://github.com/Summe-rsnow/Wonton) | 专为 Antigravity 打造的进程级独立代理与多账号额度管理工具 |
| **Sundae** | `v1.2.0` | [Summe-rsnow/Sundae](https://github.com/Summe-rsnow/Sundae) | 专为 Windows Scoop 打造的轻快原生桌面软件包管理器 |
| **Ramen** | `v1.2.0` | [Summe-rsnow/Ramen](https://github.com/Summe-rsnow/Ramen) | 专为 Windows 打造的极速桌面文件搜索与即时索引检索工具 |
| **Mango** | `v1.0.0` | [Summe-rsnow/Mango](https://github.com/Summe-rsnow/Mango) | 轻量高性能原生本地漫画阅读器 |

---

## ⚠️ 注意事项

1. **索引服务依赖与版本锁定**：Ramen 直接集成 [mustafaahci/goz](https://github.com/mustafaahci/goz) `v0.7.1` 的 `gozd.exe` 守护进程（以 `SYSTEM` 权限维护 NTFS `$MFT` 与 USN Journal 内存索引）；界面以普通用户权限运行，首次安装/启动服务时会请求一次 UAC 提权。
2. **Named Pipe 安全校验**：连接 `\\.\pipe\goz-v1` 前会强制校验管道所有者为 `SYSTEM` 或 `Administrators`，防止非特权进程抢占管道。
3. **平台支持范围**：在 macOS / Linux 上可正常编译并启动界面（提示仅支持 Windows 索引服务），实际全盘索引检索能力面向 Windows NTFS 桌面环境。

---

## 🤝 参与贡献

欢迎提交 Issue 或 Pull Request 来改进 Ramen！  
提交前请确保 `nsh check` 与 `nsh test`（或 `cargo test`）均通过且无编译警告。

## 📄 开源协议

本项目基于 [Apache License 2.0](LICENSE) 开源；集成的索引引擎组件来自 [mustafaahci/goz](https://github.com/mustafaahci/goz)（MIT 协议）。
