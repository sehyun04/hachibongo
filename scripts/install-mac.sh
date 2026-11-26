#!/bin/sh
# 하치봉고 맥 설치. 터미널에 붙여 넣는다:
#   curl -fsSL https://github.com/sehyun04/hachibongo/releases/latest/download/install-mac.sh | sh
#
# 브라우저로 받은 dmg 의 앱은 애플 공증이 없어 처음 열 때 '열지 않음' 으로 막힌다. 맥은 브라우저가 받은 파일에만
# 격리 표시(com.apple.quarantine)를 붙이고 그 표시가 있을 때만 막으므로, curl 로 받아 깔면 바로 열린다.
# 깐 뒤의 업데이트는 앱이 스스로 받으므로 그때도 막히지 않는다.
set -eu

app=hachibongo.app
url="https://github.com/sehyun04/hachibongo/releases/latest/download/hachibongo-mac.app.tar.gz"
# 관리자 계정이 아니면 /Applications 에 못 쓰므로 내 응용 프로그램 폴더에 깐다
if [ -w /Applications ]; then
  dest=/Applications
else
  dest="$HOME/Applications"
  mkdir -p "$dest"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "하치봉고 받는 중..."
curl -fsSL "$url" | tar -xzf - -C "$tmp"
test -d "$tmp/$app"

# 켜져 있으면 끄고 바꾼다. 안 켜져 있을 때 tell 을 보내면 오히려 켜지므로 먼저 확인한다.
if pgrep -xq hachibongo; then
  osascript -e 'quit app "hachibongo"' >/dev/null 2>&1 || true
  sleep 2
fi
rm -rf "$dest/$app"
mv "$tmp/$app" "$dest/$app"
# 예전에 브라우저로 받은 판에서 옮겨 붙은 표시가 있으면 지운다
xattr -dr com.apple.quarantine "$dest/$app" 2>/dev/null || true

open "$dest/$app"
echo "다 깔았어요: $dest/$app"
echo "처음 켜면 '입력 모니터링' 허락을 물어요. 시스템 설정에서 hachibongo 를 켜 주세요."
