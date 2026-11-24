<p align="center">
  <img src="images/icon.png" width="96" alt="SmartSnip">
</p>

<h1 align="center">SmartSnip</h1>

<p align="center">轻量级 Windows 常驻截图工具 · 单文件原生 exe · 零运行时依赖 · v1.0.0</p>

<p align="center">
  <a href="#特性">特性</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="#使用说明">使用说明</a> ·
  <a href="#配置">配置</a> ·
  <a href="#项目结构">项目结构</a> ·
  <a href="#开发">开发</a>
</p>

---

SmartSnip 是一款面向 Windows 10/11 的常驻截图工具，提供**框选截图、贴图便签、标注、文字识别（OCR）、二维码识别**五大能力。使用 Rust + Win32/WinRT 原生实现，产物为单个 exe，不依赖 .NET / JVM / WebView，空闲时无轮询、CPU 占用 0%。

## 特性

| 能力 | 说明 |
|---|---|
| 框选截图 | 全屏压暗遮罩、跨屏虚拟桌面、放大镜取色、`ESC` 取消、`Enter` 全屏 |
| 标注 | 画笔 / 矩形 / 文字，红黄蓝黑四色，一步撤销；标注进入截图、贴图与识别输入 |
| 贴图便签 | 多实例（上限可配）、1:1 物理像素、拖动、滚轮缩放、`Ctrl+滚轮` 透明度、双击还原、失焦淡化 |
| 文字识别 | Windows.Media.Ocr 系统引擎，零模型体积；结果面板逐行复制、一键复制全文 |
| 二维码识别 | `rqrr` 本地离线解码；文本直接入剪贴板，链接需确认后打开，高危协议附风险提示 |
| 全局热键 | 默认 `Ctrl+Alt+S`，支持设置内录制修改，冲突自动回滚并提示 |
| 系统托盘 | 左键单击立即截图；右键菜单：立即截图 / 修改快捷键 / 打开配置目录 / 开机自启 / 退出 |
| 单实例 | 命名互斥体防多开；二次启动唤醒既有实例并触发一次截图 |
| 低占用 | 事件驱动、零轮询；实测空闲 WorkingSet ≈ 12MB / 私有 ≈ 1.7MB |

## 快速开始

### 系统要求

- Windows 10 1809+ / Windows 11，x64
- 系统已安装所需 OCR 语言包（中文简体 / 英文通常自带）

### 直接运行

```powershell
# 构建
cargo build --release

# 运行
.\target\release\SmartSnip.exe
```

启动后常驻系统托盘。首次运行会在 `%USERPROFILE%\Winfxk\SmartSnip\config\` 生成默认配置。

> `debug` 构建保留控制台并输出调试信息；`release` 为窗口子系统，无控制台黑框。

### 构建前置

- Rust stable（`x86_64-pc-windows-msvc` 工具链）
- Visual Studio Build Tools 的 C++ 生成工具（提供链接器）
- 无需 Windows SDK 的 `rc.exe`：图标资源由 `build.rs` + `win-resource-builder` 以纯 Rust 生成

## 使用说明

### 截图

1. 按 `Ctrl+Alt+S`（或托盘左键、托盘菜单“立即截图”）启动遮罩。
2. 拖动框选，松开即确认；工具条出现在选区旁。
3. 选择动作：

| 按钮 | 行为 |
|---|---|
| 截图 | 将选区（含标注）复制到剪贴板 |
| 贴图 | 将选区（含标注）贴为置顶便签 |
| 识文 | 后台 OCR，结果面板弹出并自动复制全文 |
| 二维码 | 本地解码；文本入剪贴板，链接确认后打开 |
| 标注 | 进入标注模式 |

**遮罩操作**

| 操作 | 说明 |
|---|---|
| 拖动鼠标 | 框选 |
| 松开鼠标 | 确认选区 |
| 右键 / `ESC` | 取消截图 |
| `Enter` | 全屏选区并复制 |
| `C` | 复制光标处颜色 HEX |

**标注模式**：画笔 / 矩形 / 文字 / 颜色 / 撤销 / 完成。颜色在红→黄→蓝→黑间轮换；文字在点击处弹出输入框。标注保存在选区局部坐标，可随时回到主工具条执行动作。

### 贴图

| 操作 | 说明 |
|---|---|
| 拖动 | 移动贴图 |
| 滚轮 | 以光标为中心缩放（0.2×~5×） |
| `Ctrl+滚轮` | 调整透明度（40%~100%） |
| 双击 | 还原 1:1 |
| 右键 | 复制到剪贴板 / 另存为 PNG / 关闭贴图 |

贴图默认位置与截图时屏幕位置一致；失焦后可按配置自动淡化。退出程序时若仍有贴图，会二次确认。

### 设置

托盘右键 →「修改快捷键」打开设置窗口：

- 快捷键录制（录制期间暂停原热键，保存时先注册新组合，失败回滚）
- 贴图上限、初始不透明度、失焦淡化
- 开机自启
- 标注默认色
- 打开配置目录

## 配置

配置文件：`%USERPROFILE%\Winfxk\SmartSnip\config\config.toml`

```toml
[hotkey]
modifiers = ["ctrl", "alt"]
key = "s"

[pin]
max_count = 20          # 存活贴图上限（1~100）
opacity = 100           # 初始不透明度（40~100）
blur_on_unfocus = true  # 失焦淡化
blur_opacity = 70       # 淡化目标不透明度

[annotate]
color = "red"           # red / yellow / blue / black

autostart = false
log_level = "info"
autosave_dir = ""
```

- 写入采用「临时文件 + 原子替换」，配置损坏时自动备份为 `config.toml.bak` 并以默认值重建。
- `log_level` 与 `autosave_dir` 为预留字段，当前版本未使用。

日志：`...\config\logs\snip-YYYY-MM-DD.log`，按**北京时间**命名，保留 7 天、单日上限 5MB。

## 数据与隐私

- 全程本地处理，无任何联网请求（OCR 使用系统内置引擎，二维码本地解码）。
- 日志只记录事件与错误，**不记录剪贴板内容、识别文本、截图数据**。
- 粘贴内容与截图仅存在于内存与系统剪贴板。

## 架构与项目结构

```
src/
├── main.rs              入口：panic hook、日志、单实例、消息循环
├── app.rs               主窗口、DPI、全局消息常量、热键句柄、OCR 结果消费
├── overlay/             框选遮罩
│   ├── mod.rs           状态机、选区、标注、工具条动作分发
│   ├── toolbar.rs       主/标注两套工具条（原生绘制）
│   ├── text_input.rs    文字标注输入框
│   └── render.rs        放大镜与取色辅助
├── pin/mod.rs           贴图窗口：拖动/缩放/透明度/右键菜单
├── annotate.rs          标注模型与渲染（BGRA，GDI 文字）
├── ocr.rs               Windows.Media.Ocr 识别
├── qrcode.rs            rqrr 解码与跳转分流
├── capture.rs           屏幕捕获（GDI，物理像素）
├── clipboard.rs         剪贴板位图/文本、PNG 另存
├── config.rs            TOML 配置与运行时生效配置
├── logging.rs           滚动日志
├── hotkey.rs            RegisterHotKey 注册/换绑
├── tray.rs              Shell_NotifyIcon 托盘
├── autostart.rs         开机自启注册表
├── single_instance.rs   命名互斥体单实例
├── scheduler.rs         panic 隔离、硬超时、限流
├── timefmt.rs           北京时间
├── icon.rs              资源图标加载
└── ui/
    ├── settings.rs      设置窗口
    └── ocr_panel.rs     OCR 结果面板
build.rs                 生成并链接图标资源（无需 rc.exe）
images/                  icon.png / 多尺寸 icon.ico
docs/                    设计文档与实现计划
```

**设计要点**

- **事件驱动、零轮询**：热键、托盘、遮罩、贴图全部基于窗口消息。
- **原生 GDI 绘制**：无图片资源依赖，UI 随进程即用即毁，保持低内存。
- **物理像素坐标**：跨屏、高 DPI 下不做缩放合成；进程声明 per-monitor v2 DPI 感知。
- **时间统一北京时间（UTC+8）**，不依赖系统本地时区。
- **FFI 收口**：所有 `unsafe` Win32/WinRT 调用集中在系统交互模块。

## 开发

```bash
cargo build                 # debug（保留控制台）
cargo build --release       # release（窗口子系统，无控制台）
cargo run                   # 启动
cargo test                  # 全部单元测试
cargo test <测试名>          # 运行单个测试
cargo clippy --all-targets  # 静态检查
```

- 单元测试内联在各模块 `#[cfg(test)] mod tests`，覆盖配置读写、热键解析、标注渲染、时间格式、日志滚动、二维码分流等纯逻辑；UI/FFI 需真机手工验证。
- 新增源文件请保留头部版权注释；时间逻辑统一走 `timefmt`。
- 修改图标：替换 `images/icon.png` 后重新生成多尺寸 `images/icon.ico`，`cargo build` 时自动嵌入。

## 已知限制与路线图

- **屏幕捕获为 GDI**：在常规桌面下稳定；HDR 或部分硬件覆盖层内容可能偏色，DXGI Desktop Duplication 为后续可选增强（需处理静止桌面超时与 FP16 色调映射）。
- OCR 依赖系统语言包；未安装对应语言时识别为空，程序会给出安装引导。
- 仅支持 Windows。

## 常见问题

**快捷键按了没反应？**
可能与其他程序冲突。托盘会弹出提示；打开设置重新录制一个组合即可（保存失败会自动回滚旧热键）。

**OCR 识别不到文字？**
确认截图区域有清晰文字；若为其他语言，请在「设置 → 时间和语言 → 语言」安装对应语言包。

**贴图太多无法继续？**
受设置中的贴图上限限制（默认 20），关闭部分贴图或调高上限。

**配置改坏了怎么办？**
直接删除或忽略即可，程序会备份为 `config.toml.bak` 并用默认值重建。

## 许可证

本项目基于 [MIT License](LICENSE) 开源，可自由使用、修改、分发与商用，仅需保留版权声明与免责声明。

Copyright (c) 2026 Winfxk（冰月）

- Email: admin@winfxk.cn
- QQ: 2508543202
