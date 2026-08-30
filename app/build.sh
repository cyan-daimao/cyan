#!/usr/bin/env bash
# cyan 三平台打包脚本：本机平台构建 + 统一收集 zip 到项目根 bin/ 目录
#   macOS   → cyan_<ver>_mac_<arch>.zip（内含 cyan.app）+ cyan_<ver>_<arch>.dmg
#   Linux   → cyan_<ver>_linux_<arch>.zip（内含 AppImage，chmod +x 直接运行）
#   Windows → cyan_<ver>_windows_<arch>.zip（内含 cyan.exe 便携版 + NSIS 安装器）
#
# 注意：Tauri 无法交叉编译，本脚本只能产出「当前运行平台」的包；
#       三个平台的 zip 请用 .github/workflows/build.yml（Actions 一次性产出三平台产物）。
#
# 用法：
#   ./build.sh                 # 本机平台打包，zip（mac 另含 dmg）收集到 ../bin
#   ./build.sh --ci            # CI 模式（跳过图标交互确认）
#   ./build.sh -b <target>     # 透传 tauri build 参数（跳过默认收集流程）
set -euo pipefail

cd "$(dirname "$0")"

CI_MODE=0
TAURI_ARGS=()
for arg in "$@"; do
  if [[ "$arg" == "--ci" ]]; then
    CI_MODE=1
  else
    TAURI_ARGS+=("$arg")
  fi
done

ROOT_DIR="$(cd .. && pwd)"
BIN_DIR="$ROOT_DIR/bin"
BUNDLE_DIR="src-tauri/target/release/bundle"
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || echo "0.1.0")

# ---- 平台识别：mac / linux / windows（git-bash/msys 下 uname 返回 MINGW*）----
UNAME="$(uname -s)"
case "$UNAME" in
  Darwin)                OS="mac" ;;
  Linux)                 OS="linux" ;;
  MINGW*|MSYS*|CYGWIN*)  OS="windows" ;;
  *) echo "✗ 不支持的平台：$UNAME"; exit 1 ;;
esac
ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
[[ "$ARCH" == "amd64" ]] && ARCH="x86_64"

ZIP_NAME="cyan_${VERSION}_${OS}_${ARCH}.zip"
echo "==> 1/6 环境检查（平台：$OS / ${ARCH}）"
command -v node  >/dev/null || { echo "✗ 未找到 node，请先安装 Node.js 18+"; exit 1; }
command -v cargo >/dev/null || { echo "✗ 未找到 cargo，请先安装 Rust"; exit 1; }
echo "  node $(node --version) / cargo $(cargo --version | awk '{print $2}')"

# ---- Linux 系统依赖预检（webkit2gtk-4.1 等，缺了 release 编译必失败）----
if [[ "$OS" == "linux" ]]; then
  if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    echo "✗ 缺少 webkit2gtk-4.1 开发库，请先安装："
    echo "  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \\"
    echo "                 libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev"
    echo "  Fedora:        sudo dnf install webkit2gtk4.1-devel ..."
    exit 1
  fi
  echo "  webkit2gtk-4.1 ✓"
fi

echo "==> 2/6 图标检查"
ICON="src-tauri/icons/icon.png"
if [[ ! -f "$ICON" ]]; then
  echo "✗ 缺少 ${ICON}，请先生成图标：npx tauri icon /path/to/logo.png（源图 ≥512×512）"
  exit 1
fi
# sips 仅 macOS 有；其余平台跳过尺寸检查
if command -v sips >/dev/null; then
  W=$(sips -g pixelWidth  "$ICON" | awk '/pixelWidth/{print $2}')
  H=$(sips -g pixelHeight "$ICON" | awk '/pixelHeight/{print $2}')
  if [[ "${W:-0}" -lt 512 || "${H:-0}" -lt 512 ]]; then
    echo "⚠ 图标仅 ${W}×${H}（疑似占位图），打包可能失败或产出默认图标"
    if [[ "$CI_MODE" -eq 0 ]]; then
      read -r -p "  仍要继续吗？[y/N] " ans
      [[ "${ans:-N}" == "y" || "${ans:-N}" == "Y" ]] || { echo "已取消。请执行 npx tauri icon <logo.png> 后重试"; exit 1; }
    fi
  else
    echo "  图标 ${W}×${H} ✓"
  fi
fi

echo "==> 3/6 前端依赖"
if [[ ! -d node_modules ]]; then
  npm ci --no-audit --no-fund
else
  echo "  node_modules 已存在，跳过（如需干净安装请 rm -rf node_modules 后重跑）"
fi

echo "==> 4/6 打包（前端 build + Rust release + bundle，首次约 10-20 分钟）"
if [[ ${#TAURI_ARGS[@]} -gt 0 ]]; then
  # 用户显式指定 target：全权交给 tauri，不做默认收集
  npx tauri build "${TAURI_ARGS[@]}"
  echo "产物位于 $BUNDLE_DIR/"
  exit 0
fi

case "$OS" in
  # mac：只出 .app（避免 bundle_dmg.sh 挂载镜像弹窗；dmg 由下方 hdiutil 生成）
  mac)     npx tauri build -b app ;;
  # linux：AppImage 是免安装单文件，最适合装进 zip 分发
  linux)   npx tauri build -b appimage ;;
  # windows：NSIS 安装器（setup.exe）；便携版直接取 release/cyan.exe
  windows) npx tauri build -b nsis ;;
esac

echo "==> 5/6 收集产物到 $BIN_DIR"
mkdir -p "$BIN_DIR"

case "$OS" in
  mac)
    APP_PATH="$BUNDLE_DIR/macos/cyan.app"
    DMG_NAME="cyan_${VERSION}_${ARCH}.dmg"
    [[ -d "$APP_PATH" ]] || { echo "✗ 未找到 $APP_PATH"; exit 1; }
    # .app：可直接执行（未签名首次打开：右键 → 打开）
    rm -rf "$BIN_DIR/cyan.app"
    cp -R "$APP_PATH" "$BIN_DIR/"
    # .zip：ditto 保留 macOS 资源信息，分发常用格式
    rm -f "$BIN_DIR/$ZIP_NAME"
    ( cd "$BIN_DIR" && ditto -c -k --sequesterRsrc --keepParent cyan.app "$ZIP_NAME" )
    # .dmg：hdiutil 直接打包，不挂载、无弹窗
    rm -f "$BIN_DIR/$DMG_NAME"
    hdiutil create -volname "cyan" -srcfolder "$BIN_DIR/cyan.app" -ov -format UDZO "$BIN_DIR/$DMG_NAME" >/dev/null
    ;;

  linux)
    APPIMAGE="$(find "$BUNDLE_DIR/appimage" -maxdepth 1 -name '*.AppImage' 2>/dev/null | head -1)"
    [[ -n "$APPIMAGE" ]] || { echo "✗ 未找到 $BUNDLE_DIR/appimage/*.AppImage"; exit 1; }
    cp "$APPIMAGE" "$BIN_DIR/cyan_${VERSION}_linux_${ARCH}.AppImage"
    chmod +x "$BIN_DIR/cyan_${VERSION}_linux_${ARCH}.AppImage"
    # zip 命令缺失时用 python3 兜底（主流发行版二选一必有）
    rm -f "$BIN_DIR/$ZIP_NAME"
    if command -v zip >/dev/null; then
      ( cd "$BIN_DIR" && zip -q "$ZIP_NAME" "cyan_${VERSION}_linux_${ARCH}.AppImage" )
    else
      ( cd "$BIN_DIR" && python3 -m zipfile -c "$ZIP_NAME" "cyan_${VERSION}_linux_${ARCH}.AppImage" )
    fi
    echo "本机运行：./cyan_${VERSION}_linux_${ARCH}.AppImage"
    ;;

  windows)
    EXE_PATH="src-tauri/target/release/cyan.exe"
    [[ -f "$EXE_PATH" ]] || { echo "✗ 未找到 $EXE_PATH"; exit 1; }
    cp "$EXE_PATH" "$BIN_DIR/cyan.exe"
    # NSIS 安装器（存在才放入；tauri 版本/命名可能变化）
    SETUP="$(find "$BUNDLE_DIR/nsis" -maxdepth 1 -name '*-setup.exe' 2>/dev/null | head -1 || true)"
    PS_PATHS="'cyan.exe'"
    if [[ -n "${SETUP:-}" ]]; then
      cp "$SETUP" "$BIN_DIR/cyan_${VERSION}_x64-setup.exe"
      PS_PATHS="'cyan.exe','cyan_${VERSION}_x64-setup.exe'"
    fi
    # git-bash 一般无 zip 命令：用 PowerShell Compress-Archive（cd 到 bin 后用相对路径，避免路径转换问题）
    rm -f "$BIN_DIR/$ZIP_NAME"
    PS_PATHS="'cyan.exe'"
    [[ -n "${SETUP:-}" ]] && PS_PATHS="'cyan.exe','cyan_${VERSION}_x64-setup.exe'"
    ( cd "$BIN_DIR" && powershell.exe -NoProfile -Command "Compress-Archive -Path ${PS_PATHS} -DestinationPath '${ZIP_NAME}' -Force" ) >/dev/null
    echo "便携版：cyan.exe（需系统自带 WebView2 运行时，Win10/11 默认内置）"
    ;;
esac

echo "==> 6/6 完成"
echo ""
echo "产物（${BIN_DIR}）："
ls -lh "$BIN_DIR" | awk 'NR>1 {printf "  ✓ %s (%s)\n", $9, $5}'
if [[ "$OS" == "mac" ]]; then
  echo ""
  echo "本机安装：cp -R \"$BIN_DIR/cyan.app\" /Applications/"
  echo "提示：未签名包在其他 Mac 首次打开需「右键 → 打开」；分发请配置签名与公证"
fi
echo "三平台 zip 一键产出：推送后运行 .github/workflows/build.yml（Actions 产物下载 bin/*.zip）"
