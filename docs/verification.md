# 验证记录

验证日期：2026-10-26，Windows x64，本机 Rust 1.95。

## 已完成

- `cargo check --locked --all-targets`：通过。
- `cargo clippy --locked --all-targets -- -D warnings`：通过。
- `cargo test --locked`：54 项通过；1 项显式大文件测量默认跳过。
- JSON 测试包含数字原文、重复字段、Unicode、严格语法、深度与取消、范围关系、搜索截断、流式格式化、节点草稿及可见行绘制。
- JWT 测试使用合成密钥，覆盖全部 12 种算法、固定 HMAC 向量、错误密钥、篡改、算法混淆、大数及时间状态。
- 保存与历史测试包含外部修改保护、UTF-8 BOM、重启加载、来源链、收藏保留、内容去重、损坏索引和路径校验。
- Python 打包契约：22 项通过，2 项可选真实环境检查跳过。

## 性能测量

使用 `examples/json_benchmark.rs`，Windows x64、Rust 1.95、release 优化和静态 CRT，本机单次合成数据测量：

| 内容大小 | 节点数 | 节点索引占用 | 解析 | 搜索全部节点 |
| --- | ---: | ---: | ---: | ---: |
| 33,554,243 字节（约 32 MiB） | 1,792,976 | 71,719,040 字节 | 216.76 ms | 52.59 ms |
| 67,108,714 字节（约 64 MiB） | 3,585,963 | 143,438,520 字节 | 555.65 ms | 127.29 ms |

搜索在最后一条记录放置唯一关键字，遍历结束确认 1 个匹配；断言大整数原文保持。数据为包含中文、嵌套对象和高精度数字的重复记录，属于特定数据分布下的单机测量，不是所有输入和硬件的性能保证。数字不包含文件读取、历史写入、完整 GUI 首次绘制或格式化的时间。

实际桌面已打开同样约 32 MiB 的文件，显示约 179 万节点，并以虚拟行绘制约 25.6 万个顶层子项；通过第二次启动把文件转交给已有窗口，未新开第二个主界面。小文件已验证搜索折叠节点后自动展开并定位到匹配字段。

## 桌面与发布包

- Windows 便携包已实际启动，已检查 JSON / JWT 页面、中文字体、原始大整数显示、应用内搜索、历史重启加载与单实例转交文件。
- 已实际关闭主窗口，再通过悬浮球恢复原有搜索工作区；右键菜单在右下角朝左上展开，五个菜单项完整显示。
- 最后一轮加入 Windows 原生形状裁剪兜底及两项真实 GDI region 测试，排除球与菜单之外的方形空白和命中区域。该轮通过编译、严格 Clippy 和测试；用户已在操作窗口，因此没有继续抢占桌面切换新包做像素复验。
- 主窗口“完全退出”保留悬浮球的启用偏好，避免仅为退出而把下次启动设置成禁用。
- 包内程序哈希、压缩包 SHA-256、第三方许可和 OFL 字体许可已核对。当前用户安装和真实桌面 broker 的两项可选检查未在本轮执行。
- macOS、Linux 只有构建配置，尚未在目标平台实际构建或启动。

0.1.0 本机包：`dist/release-20261026/DevShop-0.1.0-windows-x64.zip`。包含最新小熊猫品牌资源和 Windows 悬浮球裁剪修正；构建前后的 60 份关键源文件/资源/脚本指纹一致，程序 SHA-256、压缩包 SHA-256 和 ZIP CRC 均通过核对。

## 1.0.0 正式版与开始菜单安装

2026-10-26 在 Windows x64 使用当前源码重新构建，执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1 -ExpectedTag v1.0.0 -Install
```

- 正式包：`dist/DevShop-1.0.0-windows-x64.zip`，旁附 `.sha256` 文件；解压目录包含程序、固定启动器、安装脚本和第三方许可。
- Windows 文件版本和产品版本均为 `1.0.0`；包内程序、启动器和 ZIP SHA-256、ZIP CRC 均核对通过。
- 已通过真实 Explorer 桌面代理安装到 `%LOCALAPPDATA%\Programs\Shop\DevShop`，并添加 `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Shop\DevShop.lnk`。快捷方式目标、图标和 `current.path` 均已核对。
- 开始菜单注册入口已实际启动 1.0.0 安装目录下的程序，返回 `DevShop` 主窗口及快捷入口窗口。Computer Use 的窗口绑定报告 `window id 25168950 no longer belongs to ...; current owner is ...`，因此本轮未完成截图复核。
- `scripts/dev.ps1 test`：62 项通过，1 项大文件测量按默认设置跳过。
- Python 打包与安装测试：24 项全部通过，包括启用真实 Explorer 只读 roundtrip，以及使用本次编译启动器的参数转发和版本切换测试。
- `python scripts/check-structure.py`：通过，检查 59 个文件。
- 本地发布与安装核对结果：`target/qa/release-1.0.0-audit.json`。

ZIP SHA-256：`046be943f7f5fd481dbc068190d56e9a4c4534e49b9462b5de8188b0e1685fc6`。
