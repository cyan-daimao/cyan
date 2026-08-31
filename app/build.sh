#!/usr/bin/env bash
# cyan 打包脚本：统一收集产物到项目根 bin/ 目录
#
# 用法：
#   ./build.sh            → 本机平台安装包（mac: app/zip/dmg；linux: AppImage/zip；windows: nsis/zip）
#   ./build.sh --win      → 在 mac/linux 上交叉编译 Windows 便携版 exe（免安装、无安装器）
#                           产出 bin/cyan.exe + cyan_<ver>_windows_x86_64.zip
#   ./build.sh --all      → 一键双平台：本机平台 zip + Windows 便携版 zip
#                           （mac 上 = mac zip/dmg + win zip；linux 上 = linux zip + win zip）
#   ./build.sh --ci       → CI 模式（跳过图标交互确认；可与其他参数组合）
#   ./build.sh -b <args>  → 透传 tauri build 参数（跳过默认收集流程）
#
# 产物命名：cyan_<version>_<platform>_<arch>.zip
# 说明：tauri 无法交叉编译「安装器」；--win/--all 的 Windows 产物为便携 exe（cargo-xwin，
#       MSVC target），目标机需已装 WebView2 运行时（Win10/11 默认内置）。
#       正式分发（含 NSIS 安装器）建议 CI 三平台原生构建：.github/workflows/build.yml。
set -euo pipefail

cd "$(dirname "$0")"

# ---- 参数解析 ----
CI_MODE=0
WANT_WIN=0     # 交叉编译 Windows 便携版
WANT_LOCAL=0   # 本机平台构建
WANT_ALL=0     # 本机 + Windows 一起出
WANT_LOCAL=1   # 默认构建本机平台（--win 时置 0）
TAURI_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --ci)  CI_MODE=1 ;;
    --win) WANT_WIN=1; WANT_LOCAL=0 ;;
    --all) WANT_ALL=1; WANT_WIN=1 ;;
    *)     TAURI_ARGS+=("$arg") ;;
  esac
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
  *) echo "✗ 不支持的平台：${UNAME}"; exit 1 ;;
esac
ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
[[ "$ARCH" == "amd64" ]] && ARCH="x86_64"

# homebrew 的 llvm/lld 自动入 PATH（交叉编译 Windows 需要 clang-cl/lld-link，
# 默认不在 PATH；lld-link 在独立的 lld 包）
for d in /opt/homebrew/opt/llvm/bin /opt/homebrew/opt/lld/bin \
         /usr/local/opt/llvm/bin  /usr/local/opt/lld/bin; do
  [[ -d "$d" ]] && PATH="$d:$PATH"
done
export PATH

# ---- 产物汇总 ----
summary() {
  echo ""
  echo "产物（${BIN_DIR}）："
  ls -lh "$BIN_DIR" | awk 'NR>1 {printf "  ✓ %s (%s)\n", $9, $5}'
}

# ---- mac 本机构建：.app + ditto zip + hdiutil dmg ----
build_mac() {
  echo "==> [mac] 打包 .app（前端 build + Rust release，首次约 10-20 分钟）"
  # 只出 .app：避免 bundle_dmg.sh 挂载镜像弹窗；dmg 由下方 hdiutil 生成
  npx tauri build -b app

  echo "==> [mac] 收集产物到 ${BIN_DIR}"
  mkdir -p "$BIN_DIR"
  local APP_PATH="$BUNDLE_DIR/macos/cyan.app"
  local DMG_NAME="cyan_${VERSION}_${ARCH}.dmg"
  [[ -d "$APP_PATH" ]] || { echo "✗ 未找到 ${APP_PATH}"; exit 1; }
  # .app：可直接执行（未签名首次打开：右键 → 打开）
  rm -rf "$BIN_DIR/cyan.app"
  cp -R "$APP_PATH" "$BIN_DIR/"
  # .zip：ditto 保留 macOS 资源信息，分发常用格式
  rm -f "$BIN_DIR/cyan_${VERSION}_mac_${ARCH}.zip"
  ( cd "$BIN_DIR" && ditto -c -k --sequesterRsrc --keepParent cyan.app "cyan_${VERSION}_mac_${ARCH}.zip" )
  # .dmg：hdiutil 直接打包，不挂载、无弹窗
  rm -f "$BIN_DIR/${DMG_NAME}"
  hdiutil create -volname "cyan" -srcfolder "$BIN_DIR/cyan.app" -ov -format UDZO "$BIN_DIR/${DMG_NAME}" >/dev/null
  echo "  mac zip/dmg ✓"
}

# ---- Windows 便携版（mac/linux 交叉编译 x86_64）：免安装 exe，无安装器 ----
build_win_portable() {
  echo "==> [win] 交叉编译 Windows 便携版（cargo-xwin，首次需下载 Windows SDK）"
  local WIN_TARGET="x86_64-pc-windows-msvc"
  # 1) rust target
  local INSTALLED_TARGETS
  INSTALLED_TARGETS="$(rustup target list --installed 2>/dev/null || true)"
  if [[ "$INSTALLED_TARGETS" != *"$WIN_TARGET"* ]]; then
    echo "  安装 rust target：${WIN_TARGET}"
    rustup target add "$WIN_TARGET"
  fi
  # 2) cargo-xwin（下载 Windows SDK/CRT 并驱动 clang 交叉链接）
  if ! command -v cargo-xwin >/dev/null; then
    echo "  安装 cargo-xwin（首次）..."
    cargo install cargo-xwin --locked
  fi
  # 3) llvm 工具链（clang-cl / lld-link；Apple clang 不含，lld-link 在 lld 包）
  if ! command -v clang-cl >/dev/null || ! command -v lld-link >/dev/null; then
    echo "✗ 交叉编译需要 llvm 工具链（clang-cl / lld-link）："
    echo "    brew install llvm lld"
    exit 1
  fi
  # 4) 只编译不产安装器（--no-bundle），得到裸 exe（tauri-cli ≥2.x 用 --no-bundle 取代 -b none）
  npx tauri build --runner cargo-xwin --target "$WIN_TARGET" --no-bundle
  local WIN_EXE="src-tauri/target/${WIN_TARGET}/release/cyan.exe"
  [[ -f "$WIN_EXE" ]] || { echo "✗ 未找到 ${WIN_EXE}"; exit 1; }

  echo "==> [win] 收集产物到 ${BIN_DIR}"
  mkdir -p "$BIN_DIR"
  cp "$WIN_EXE" "$BIN_DIR/cyan.exe"
  # WebView2Loader.dll：构建若产出则一并附带（部分系统定位运行时需要）
  local LOADER
  LOADER="$(find "src-tauri/target/${WIN_TARGET}/release" -maxdepth 1 -name 'WebView2Loader.dll' 2>/dev/null | head -1 || true)"
  local WIN_ZIP="cyan_${VERSION}_windows_x86_64.zip"
  rm -f "$BIN_DIR/${WIN_ZIP}"
  if [[ -n "${LOADER:-}" ]]; then
    cp "$LOADER" "$BIN_DIR/WebView2Loader.dll"
    ( cd "$BIN_DIR" && { command -v zip >/dev/null && zip -q "$WIN_ZIP" cyan.exe WebView2Loader.dll || python3 -m zipfile -c "$WIN_ZIP" cyan.exe WebView2Loader.dll; } )
  else
    ( cd "$BIN_DIR" && { command -v zip >/dev/null && zip -q "$WIN_ZIP" cyan.exe || python3 -m zipfile -c "$WIN_ZIP" cyan.exe; } )
  fi
  echo "  win 便携版 zip ✓（需系统 WebView2 运行时，Win10/11 默认内置；未签名首启过 SmartScreen）"
}

# ---- linux 本机构建：AppImage + zip ----
build_linux() {
  echo "==> [linux] 打包 AppImage（前端 build + Rust release，首次约 10-20 分钟）"
  npx tauri build -b appimage

  echo "==> [linux] 收集产物到 ${BIN_DIR}"
  mkdir -p "$BIN_DIR"
  local APPIMAGE
  APPIMAGE="$(find "$BUNDLE_DIR/appimage" -maxdepth 1 -name '*.AppImage' 2>/dev/null | head -1)"
  [[ -n "$APPIMAGE" ]] || { echo "✗ 未找到 $BUNDLE_DIR/appimage/*.AppImage"; exit 1; }
  local LINUX_EXE="cyan_${VERSION}_linux_${ARCH}.AppImage"
  cp "$APPIMAGE" "$BIN_DIR/${LINUX_EXE}"
  chmod +x "$BIN_DIR/${LINUX_EXE}"
  # zip 命令缺失时用 python3 兜底（主流发行版二选一必有）
  rm -f "$BIN_DIR/cyan_${VERSION}_linux_${ARCH}.zip"
  if command -v zip >/dev/null; then
    ( cd "$BIN_DIR" && zip -q "cyan_${VERSION}_linux_${ARCH}.zip" "$LINUX_EXE" )
  else
    ( cd "$BIN_DIR" && python3 -m zipfile -c "cyan_${VERSION}_linux_${ARCH}.zip" "$LINUX_EXE" )
  fi
  echo "  linux zip ✓（本机运行：./${LINUX_EXE}）"
}

# ---- windows 本机构建（git-bash）：NSIS 安装器 + 便携 zip ----
build_windows_native() {
  echo "==> [win] 原生打包 NSIS（前端 build + Rust release，首次约 10-20 分钟）"
  npx tauri build -b nsis

  echo "==> [win] 收集产物到 ${BIN_DIR}"
  mkdir -p "$BIN_DIR"
  local EXE_PATH="src-tauri/target/release/cyan.exe"
  [[ -f "$EXE_PATH" ]] || { echo "✗ 未找到 ${EXE_PATH}"; exit 1; }
  cp "$EXE_PATH" "$BIN_DIR/cyan.exe"
  # NSIS 安装器（存在才放入；tauri 版本/命名可能变化）
  local SETUP
  SETUP="$(find "$BUNDLE_DIR/nsis" -maxdepth 1 -name '*-setup.exe' 2>/dev/null | head -1 || true)"
  if [[ -n "${SETUP:-}" ]]; then
    cp "$SETUP" "$BIN_DIR/cyan_${VERSION}_x64-setup.exe"
  fi
  # git-bash 一般无 zip 命令：用 PowerShell Compress-Archive（cd 到 bin 后用相对路径，避免路径转换问题）
  rm -f "$BIN_DIR/cyan_${VERSION}_windows_${ARCH}.zip"
  local PS_PATHS="'cyan.exe'"
  [[ -n "${SETUP:-}" ]] && PS_PATHS="'cyan.exe','cyan_${VERSION}_x64-setup.exe'"
  ( cd "$BIN_DIR" && powershell.exe -NoProfile -Command "Compress-Archive -Path ${PS_PATHS} -DestinationPath 'cyan_${VERSION}_windows_${ARCH}.zip' -Force" ) >/dev/null
  echo "  win zip ✓（便携版需系统自带 WebView2 运行时）"
}

echo "==> 1/4 环境检查（平台：${OS} / ${ARCH}；目标：${WANT_LOCAL:+本机 }${WANT_WIN:++ Windows 便携版}）"
command -v node  >/dev/null || { echo "✗ 未找到 node，请先安装 Node.js 18+"; exit 1; }
command -v cargo >/dev/null || { echo "✗ 未找到 cargo，请先安装 Rust"; exit 1; }
echo "  node $(node --version) / cargo $(cargo --version | awk '{print $2}')"

# ---- Linux 系统依赖预检（webkit2gtk-4.1 等，缺了 release 编译必失败；仅本机 linux 构建需要）----
if [[ "$OS" == "linux" && "$WANT_LOCAL" -eq 1 ]]; then
  if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    echo "✗ 缺少 webkit2gtk-4.1 开发库，请先安装："
    echo "  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \\"
    echo "                 libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev"
    echo "  Fedora:        sudo dnf install webkit2gtk4.1-devel ..."
    exit 1
  fi
  echo "  webkit2gtk-4.1 ✓"
fi

echo "==> 2/4 图标检查"
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

echo "==> 3/4 前端依赖"
if [[ ! -d node_modules ]]; then
  npm ci --no-audit --no-fund
else
  echo "  node_modules 已存在，跳过（如需干净安装请 rm -rf node_modules 后重跑）"
fi

# 用户显式透传 tauri 参数：全权交给 tauri，不做默认收集
if [[ ${#TAURI_ARGS[@]} -gt 0 ]]; then
  echo "==> 4/4 透传参数给 tauri build：${TAURI_ARGS[*]}"
  npx tauri build "${TAURI_ARGS[@]}"
  echo "产物位于 ${BUNDLE_DIR}/"
  exit 0
fi

echo "==> 4/4 打包"
if [[ "$WANT_ALL" -eq 1 && "$OS" == "windows" ]]; then
  # Windows 本机原生构建已产出 win 包，无需交叉
  WANT_WIN=0
fi

if [[ "$WANT_LOCAL" -eq 1 ]]; then
  case "$OS" in
    mac)     build_mac ;;
    linux)   build_linux ;;
    windows) build_windows_native ;;
  esac
fi
if [[ "$WANT_WIN" -eq 1 && "$OS" != "windows" ]]; then
  build_win_portable
fi

summary
echo ""
if [[ "$WANT_LOCAL" -eq 1 && "$OS" == "mac" ]]; then
  echo "本机安装：cp -R \"$BIN_DIR/cyan.app\" /Applications/"
  echo "提示：未签名包在其他 Mac 首次打开需「右键 → 打开」；分发请配置签名与公证"
fi
if [[ "$WANT_WIN" -eq 1 && "$OS" != "windows" ]]; then
  echo "便携版：${BIN_DIR}/cyan.exe 免安装，拷到 Windows 即用；如需 NSIS 安装器请用 .github/workflows/build.yml 原生构建"
fi
if [[ "$WANT_ALL" -eq 0 && "$WANT_WIN" -eq 0 ]]; then
  echo "一键双平台（mac + win zip）：./build.sh --all"
fi
