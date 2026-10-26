# Windows 用户安装与文件打开方式

DevShop 沿用 Shop 系列的当前用户安装流程。需要 Windows、Python 3.9+、正在运行的 Explorer，以及与本机架构一致的 Windows MSVC 包；不需要管理员权限。

## 首次安装

从源码打包并安装：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package.ps1 -Install -OutputDirectory .\dist\installed-candidate
```

已有完整解压包时，在包内运行 `install.ps1`；也可从仓库指定完整包路径：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -PackageDirectory 'D:\Packages\DevShop-1.0.0-windows-x64'
```

安装会创建开始菜单 `Shop/DevShop`，并将 `.json`、`.jwt` 注册到“打开方式”。不会更改已有默认应用或受保护的 `UserChoice`。

## 后续更新

当前源码仓库首次执行 `-Install` 后，每次普通的成功打包都会自动刷新这个仓库对应的安装。CI、跨平台包、失败的打包和其他仓库检出不会更新该安装。加 `-NoInstall` 可跳过；同版本再次打包需要新的输出目录，现有包不会被覆盖。

开始菜单和注册表中的打开命令始终指向固定启动器：

```text
%LOCALAPPDATA%\Programs\Shop\DevShop\
  DevShop.exe
  current.path
  install-state.json
  versions\<version>-<exe-sha>-<package-sha>\
    devshop.exe
    package.json
    THIRD-PARTY-NOTICES.tsv
    licenses\ ...
%APPDATA%\Microsoft\Windows\Start Menu\Programs\Shop\DevShop.lnk
```

安装先校验并复制整个包，再原子切换 `current.path`。已运行的窗口和悬浮球保留当前版本；从悬浮球“完全退出”后重新启动才使用新版。安装器不强杀进程，不替换正在运行的程序，也不删除旧版本。

安装通过真实 Explorer 桌面对象启动隐藏的 Python 代理，避免从 MSIX 工具运行时仅写入工具私有的 AppData/HKCU 视图。代理验证真实开始菜单路径并返回完成状态；父进程不会提前报告安装成功。

## 打开文件与单实例

在文件右键菜单中选择“打开方式 → DevShop”，或传入路径：

```powershell
.\devshop.exe --open 'C:\data\example.json'
```

Windows 下同一用户登录会话只保留一个 DevShop 图形实例。再次启动时，文件会转交给已有窗口，并恢复主窗口；关闭主窗口而保留悬浮球时仍属于同一实例。已有内容的确认与保存由工作台处理。

## 验证

```powershell
python -m unittest discover -s scripts/tests -v
python scripts/check-structure.py
```

测试覆盖包校验、稳定入口更新、含空格和中文的路径、不修改默认应用的注册契约，以及真实桌面代理的边界。真实 Explorer roundtrip 和编译后启动器 smoke test 为显式启用的附加检查；默认测试不会操作真实安装。
