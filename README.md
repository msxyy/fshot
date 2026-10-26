# fshot

轻量截图 + 标注工具（Windows），托盘常驻 + 全局热键。

## 下载

从 [Releases](https://github.com/msxyy/fshot/releases/latest) 下载最新的 `fshot.exe`（Windows x64）。

> 未签名 exe 首次运行可能被 SmartScreen 拦截，选择“更多信息 → 仍要运行”。

## 特性（规划）
- 区域 / 窗口智能截图
- 对象化标注：矩形、箭头、文本、画笔、序号、马赛克
- 撤销 / 重做
- 贴图（Pin）置顶浮窗
- 离线 OCR
- 复制到剪贴板 / Ctrl+S 保存

## 构建
cargo run --release