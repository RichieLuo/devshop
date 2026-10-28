# DevShop 图标

DevShop 使用暖橙色小熊猫头像，与 LinkShop 的蓝色狐狸、RPShop 的紫色猫头鹰、SQLShop 的绿色猞猁组成同一系列。共同特征是正面动物头像、利落色块、浅色圆角底板和清晰的眼部轮廓。

小熊猫头部在定稿时缩小约 10%，让耳朵、脸颊和下巴周围保留更多底板空间；外层图标没有额外增加透明留白。

- 原始素材：`assets/branding/devshop-red-panda.png`。
- README 横版标志：`assets/branding/devshop-red-panda-logo.png`，采用小熊猫头像与深色粗体 DevShop 字标的左右布局、近白背景，不带应用图标底板；与 LinkShop、RPShop、SQLShop 一样使用 1879×837 画布，在 README 中居中显示为 560px 宽。
- 规范化应用图标：`assets/branding/devshop-icon.png`，保留浅色圆角底板，用于桌面应用展示。
- `src/branding_icon.rs::icon_image()` 统一生成 1024×1024 图标，裁去源图透明留白、排除极淡杂点，并应用与其他 Shop 一致的圆角遮罩。
- Windows 的底板填满画布，避免桌面和任务栏图标偏小；其他平台保留 10% 外边距。
- EXE 中的 16、20、24、32、40、48、64、128、256px ICO，以及运行中窗口、任务栏、标题栏的纹理，均来自同一套像素。20px 和 40px 为 125% DPI 提供原生尺寸。

图标测试逐档检查可见范围、实心中心与透明圆角。设置 `SHOP_ICON_PREVIEW` 可将规范化图标导出为 PNG。正式 Windows 包位于 `dist/icon-size-20261026/`。

## 任务栏透明圆角

运行中的窗口通过 `src/ui/window_chrome/icons.rs` 直接从 EXE 的多尺寸 ICO 资源加载 `ICON_SMALL` 和 `ICON_BIG`，按当前屏幕 DPI 选择尺寸，避免把单张 256px RGBA 图标交给 Windows 再缩小后产生灰黑色圆角。`WM_DPICHANGED` 会重新加载对应尺寸，窗口销毁时释放原生图标句柄。

图标像素仍采用预乘透明度进行重采样；窗口内的标题栏和悬浮球纹理继续走现有缓存。原生回归测试检查 100%、125%、150%、200% DPI 下的实际 HICON 尺寸、半透明圆角颜色、浅色背景上的实际绘制结果和 DPI 切换。修复包输出到 `dist/taskbar-alpha-20261026-final/`。
