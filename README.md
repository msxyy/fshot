# fshot

轻量截图 + 标注工具（Windows），托盘常驻 + 全局热键。

## 下载

从 [Releases](https://github.com/msxyy/fshot/releases/latest) 下载最新的 `fshot.exe`（Windows x64）。

> 未签名 exe 首次运行可能被 SmartScreen 拦截，选择“更多信息 → 仍要运行”。

## 特性

- 多显示器全屏截图；`Ctrl+Shift+W` 截取当前前台窗口
- 选区拖拽、边框移动、8 个控制点缩放、「⟲ 重选」
- 标注：矩形、椭圆、箭头、直线、画笔、文本、序号、马赛克
- 文本双击再次编辑；撤销 / 重做；Delete 删除选中标注
- 多屏同时框选：确认后按虚拟桌面物理坐标合并为一张图
- 设置界面：截图热键、窗口截图热键、完成后动作、保存目录
- 贴图（Pin）：置顶浮窗，左键拖动，右键 / Esc 关闭
- 托盘菜单；配置持久化到 `%APPDATA%\fshot\config.json`
- 计划中：离线 OCR、贴图缩放/旋转、安装包与签名

## 使用

1. 启动 `fshot.exe`，程序常驻托盘。
2. 按 `Ctrl+Shift+S` 全屏截图，或 `Ctrl+Shift+W` 截取当前窗口。
3. 拖出选区后标注；`Enter` 按「截图完成后」设置处理，`Ctrl+S` 保存 PNG，`Esc` 取消。
4. 托盘菜单「设置…」修改热键和保存行为；也可以用 `fshot.exe --settings` 直接打开设置。

## 构建

```powershell
cargo build --release
```

产物：`target\release\fshot.exe`。
