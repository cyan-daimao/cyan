#!/usr/bin/env bash
# cyan 打包脚本：前端构建 + Rust release 编译 + 平台安装包
# macOS 产物（.app / .zip / .dmg）统一收集到项目根 bin/ 目录
# dmg 由 hdiutil 直接生成，不挂载镜像、不弹出任何窗口
# 用法：
#   ./build.sh                 # 打包 app + zip + dmg 到 ../bin
#   ./build.sh --ci            # CI 模式（跳过图标交互确认）
#   ./build.sh -b <target>     # 透传 tauri build 参数（会跳过默认收集流程）
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
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || echo "0.1.0")
ARCH=$(uname -m) # arm64 / x86_64

echo "==> 1/5 环境检查"
command -v node  >/dev/null || { echo "✗ 未找到 node，请先安装 Node.js 18+"; exit 1; }
command -v cargo >/dev/null || { echo "✗ 未找到 cargo，请先安装 Rust"; exit 1; }
echo "  node $(node --version) / cargo $(cargo --version | awk '{print $2}') / $ARCH"

echo "==> 2/5 图标检查"
ICON="src-tauri/icons/icon.png"
if [[ ! -f "$ICON" ]]; then
  echo "✗ 缺少 $ICON，请先生成图标：npx tauri icon /path/to/logo.png（源图 ≥512×512）"
  exit 1
fi
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

echo "==> 3/5 前端依赖"
if [[ ! -d node_modules ]]; then
  npm ci --no-audit --no-fund
else
  echo "  node_modules 已存在，跳过（如需干净安装请 rm -rf node_modules 后重跑）"
fi

echo "==> 4/5 打包（前端 build + Rust release + bundle，首次约 10-20 分钟）"
if [[ ${#TAURI_ARGS[@]} -gt 0 ]]; then
  # 用户显式指定 target：全权交给 tauri，不做默认收集
  npx tauri build "${TAURI_ARGS[@]}"
  echo "产物位于 src-tauri/target/release/bundle/"
  exit 0
fi

if [[ "$(uname)" != "Darwin" ]]; then
  npx tauri build
  echo "非 macOS 平台，产物位于 src-tauri/target/release/bundle/"
  exit 0
fi

# macOS 默认流程：tauri 只出 .app（不带 dmg，避免 bundle_dmg.sh 挂载镜像弹窗）
npx tauri build -b app

echo "==> 5/5 收集产物到 $BIN_DIR"
BUNDLE_DIR="src-tauri/target/release/bundle"
APP_PATH="$BUNDLE_DIR/macos/cyan.app"
ZIP_NAME="cyan_${VERSION}_${ARCH}.zip"
DMG_NAME="cyan_${VERSION}_${ARCH}.dmg"
mkdir -p "$BIN_DIR"

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

echo ""
echo "产物（${BIN_DIR}）："
ls -lh "$BIN_DIR" | awk 'NR>1 {printf "  ✓ %s (%s)\n", $9, $5}'
echo ""
echo "本机安装：cp -R \"$BIN_DIR/cyan.app\" /Applications/"
echo "提示：未签名包在其他 Mac 首次打开需「右键 → 打开」；分发请配置签名与公证"
