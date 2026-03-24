# Glance

一款轻量级桌面截图翻译工具。快捷键截屏，框选即翻译，基于有道 OCR 实现实时图片翻译。

使用 Tauri 2 + 原生窗口渲染，启动快、体积小。

## 功能

- 全局快捷键一键截图（默认 `Ctrl+Shift+X`）
- 框选区域后自动调用有道 OCR 翻译
- 翻译结果直接覆盖在原图位置
- 支持多语言互译（中/英/日/韩/法/德/俄/西）
- 翻译历史记录
- 系统托盘常驻，关闭窗口不退出
- 开机自启（可选）

## 截图

> TODO

## 技术栈

- **后端**：Rust + Tauri 2
- **前端**：原生 HTML / CSS / JavaScript（无框架）
- **截图**：Windows BitBlt（通过 `screenshots` crate）
- **选区窗口**：winit + softbuffer 原生渲染（无 WebView 开销）
- **翻译 API**：有道智云 OCR 图片翻译

## 项目结构

```
src-tauri/
├── src/
│   ├── main.rs            # 应用入口，托盘 & 窗口管理
│   ├── commands.rs        # Tauri 命令（截图、翻译、设置）
│   ├── capture.rs         # 屏幕截图（BitBlt）
│   ├── capture_window.rs  # 原生全屏选区窗口（winit + softbuffer）
│   ├── api.rs             # 有道翻译 API 客户端
│   ├── app_state.rs       # 全局状态管理
│   ├── config.rs          # 配置持久化
│   ├── models.rs          # 数据类型定义
│   └── error.rs           # 错误处理
├── icons/                 # 应用图标
└── Cargo.toml
ui/
├── index.html             # 主窗口
├── overlay.html           # 翻译结果浮层
├── styles.css             # 样式
└── app.js                 # 前端逻辑
```

## 开发

需要 Rust 工具链和 Tauri CLI：

```bash
cargo install tauri-cli --version "^2"
```

开发运行：

```bash
cargo tauri dev
```

打包：

```bash
cargo tauri build
```

产物在 `src-tauri/target/release/bundle/nsis/Glance_x.x.x_x64-setup.exe`。

## 许可证

MIT
