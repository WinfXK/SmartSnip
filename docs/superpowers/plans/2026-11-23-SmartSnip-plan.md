# SmartSnip 实现任务清单

> **执行方式**：按任务顺序逐个实现，每个任务完成后可独立验证。
> **规格**：`docs/superpowers/specs/2026-11-23-SmartSnip-design.md`

**Goal**：Windows 常驻截图工具（截图/贴图/OCR/二维码），Rust 单 exe，空闲内存 ≤15MB。

**Tech Stack**：Rust + windows-rs（Win32/WinRT）+ tray-icon + arboard + rqrr + toml

## Global Constraints

- 单进程单 exe；无 WebView/Electron；空闲 CPU 0%、内存目标 ≤15MB
- 安装目录 `C:\Users\{UserName}\Winfxk\SmartSnip\`（含 exe 与 config）
- 自启：`HKCU\...\CurrentVersion\Run`
- 版权头：Winfxk（冰月），admin@winfxk.cn / QQ 2508543202 + 创建时间；时间逻辑一律北京时间
- 禁止 `unsafe` 扩散，FFI 收口到 sys 交互模块

## 任务清单

### Task 0：项目骨架
- [ ] git init + .gitignore
- [ ] cargo init（bin），版权头模板、license 模块、北京时间时间模块（time 格式化用 chrono，固定 UTC+8）
- [ ] Cargo.toml 全量依赖声明（windows features 按需最小化）
- [ ] 空窗口可启动（winit 消息循环骨架）
- 验收：`cargo build --release` 通过，exe 可运行退出

### Task 1：配置与持久化模块
- Files: `src/config.rs`、`config/config.toml`（运行时生成）
- Produces: `Config` 结构体（hotkey、pin_max=20、pin_opacity、blur_on_unfocus=true、colors、autosave_dir）、`load()` 容错（损坏→.bak+默认）、`save()` 原子写（tmp+rename）
- 验收：单测覆盖 roundtrip、损坏恢复

### Task 2：单实例 + IPC 唤醒
- Files: `src/single_instance.rs`
- Produces: `acquire() -> bool`（命名 Mutex `SmartSnip_SingleInstance`）；二次启动经命名管道发送 `activate` 命令后退出；主进程监听收到后触发一次截图
- 验收：双开时第二个进程退出且首实例收到事件

### Task 3：托盘 + 自启管理
- Files: `src/tray.rs`、`src/autostart.rs`
- Produces: 托盘图标（左键→截图事件；右键菜单：立即截图/修改快捷键/设置/打开配置目录/开机自启(勾选态)/退出）；`Autostart::enable()/disable()/is_enabled()`（注册表 Run 键）
- 验收：菜单项事件可达主循环；自启键值读写正确

### Task 4：全局快捷键引擎
- Files: `src/hotkey.rs`
- Produces: `register(combo: HotKeyCombo) -> Result<(), RegisterError::Conflict>`；默认 Ctrl+Alt+S；冲突→气泡提示+打开设置；修改流程先试注册成功才落盘
- 验收：注册/注销/冲突检测单测 + 手工验证

### Task 5：屏幕捕获模块
- Files: `src/capture.rs`
- Produces: `capture_region(Rect physical_px) -> Result<Frame>`；枚举显示器 `monitors() -> Vec<MonitorInfo>`（含物理尺寸/位置/DPI）；DXGI Desktop Duplication 首选，失败降级 GDI BitBlt
- 验收：抓取指定区域位图正确（多屏坐标换算单测）

### Task 6：框选覆盖层
- Files: `src/overlay/mod.rs`、`overlay/render.rs`、`overlay/magnifier.rs`
- Produces: 全屏跨显示器遮罩（压暗+十字光标+选区高亮+尺寸提示+放大镜含颜色值）、窗口吸附（Win32 窗口矩形检测）、ESC/Enter 退出、松开鼠标确认 → 输出 `Selection { rect, monitor_id }`
- 验收：真机交互验证（多屏、高 DPI）

### Task 7：截图写剪贴板 + 另存
- Files: `src/clipboard.rs`
- Produces: `copy_bitmap(Frame+annotations)`（PNG+DIB 双写）、`copy_text`、`save_png(path)`；复制成功轻提示浮窗
- 验收：微信/QQ/画图粘贴验证

### Task 8：标注模块（基础三件套）
- Files: `src/annotate.rs`
- Produces: `AnnotationLayer`（pen 画笔路径/rect/Text）、四色（红黄蓝黑）、undo 一步、渲染到 Frame；工具条 UI 切换状态机
- 验收：标注叠加后截图与贴图均含标注；undo 正确

### Task 9：贴图窗口（多实例）
- Files: `src/pin/mod.rs`、`pin/window.rs`
- Produces: `PinWindow::create(Frame, screen_pos)`；TOPMOST 无边框 1:1 物理像素；悬停工具钮（关闭/复制/另存/还原标注）、拖动、滚轮缩放 0.2~5×、Ctrl+滚轮透明度 40~100%、失焦淡化(可选)；上限 20 张
- 验收：多张并存、缩放/透明度/拖动真机验证；内存即用即毁

### Task 10：OCR（WinRT）
- Files: `src/ocr.rs`、`src/ui/ocr_panel.rs`
- Produces: `recognize(Frame) -> Result<Vec<Line>>`（Windows.Media.Ocr，优先 zh-Hans/en）；结果浮窗（全部复制 / 手动选择按行摘取）；空结果→语言包引导提示；后台线程+10s 硬超时
- 验收：中英截图识别正确；无语言包场景提示正确

### Task 11：二维码识别
- Files: `src/qrcode.rs`、`src/ui/qr_dialog.rs`
- Produces: `decode(Frame) -> Option<QrContent>`（rqrr 离线）；文本→直接剪贴板+浮窗；URL/App 协议→确认框展示完整地址后才打开；高危 scheme（file://、ms-msdt: 等）附加风险提示
- 验收：URL 二维码必须确认后跳转；文本二维码直达剪贴板

### Task 12：颜色拾取
- Files: `src/overlay/magnifier.rs`（扩展）
- Produces: 放大镜面板显 HEX/RGB，选区状态下按 C 复制颜色（不截屏退出）
- 验收：拾取值与实际像素一致

### Task 13：设置窗口 + 快捷键录制
- Files: `src/ui/settings.rs`
- Produces: 设置窗口（快捷键录制控件：修饰键≥1 校验、冲突试注册）、贴图参数、自启开关、日志级别
- 验收：修改热键全流程（冲突→提示→改成功→生效）

### Task 14：调度与容错
- Files: `src/scheduler.rs`、panic hook
- Produces: 捕获通道自动降级/恢复、OCR 限流（<500ms 合并）、贴图内存预算 512MB、panic 落盘不整进程崩溃、错误提示三原则（合并 60s 同类）
- 验收：注入故障后自动降级并恢复；24h 常驻内存增长 ≤2MB

### Task 15：日志 + 收尾
- Files: `src/logging.rs`
- Produces: 滚动日志（logs/snip-YYYY-MM-DD.log、7 天/5MB 清理、不落盘剪贴板与识别内容）；打包发布脚本（release + 便携 zip）
- 验收：验收基线全项通过（热键≤100ms、框选→出图≤300ms、OCR≤2s）
