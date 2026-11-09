#!/usr/bin/env bash
# 一键部署整站到 Cloudflare Workers（glassmc.shuan.uk）
#
#   /      介绍页
#   /web   在线体验
#
# 令牌来源优先级：环境变量 CLOUDFLARE_API_TOKEN > 钥匙串 glassmc-site-token
# 首次使用先存一次（令牌在 Cloudflare 控制台 → 我的个人资料 → API 令牌创建，
# 模板选「Edit Cloudflare Workers」）：
#   security add-generic-password -a cloudflare -s glassmc-site-token -w cfat_xxx -U
set -euo pipefail
cd "$(dirname "$0")/.."

ACCOUNT_ID=80aca1f1efb175ea21fbf99eda06b786

TOKEN="${CLOUDFLARE_API_TOKEN:-}"
if [ -z "$TOKEN" ]; then
  TOKEN=$(security find-generic-password -a cloudflare -s glassmc-site-token -w 2>/dev/null || true)
fi
if [ -z "$TOKEN" ]; then
  echo "找不到 Cloudflare 令牌。二选一："
  echo "  export CLOUDFLARE_API_TOKEN=cfat_xxx"
  echo "  security add-generic-password -a cloudflare -s glassmc-site-token -w cfat_xxx -U"
  exit 1
fi

bash Scripts/build-site.sh

cd demo
CLOUDFLARE_API_TOKEN="$TOKEN" \
CLOUDFLARE_ACCOUNT_ID="$ACCOUNT_ID" \
CI=true WRANGLER_SEND_METRICS=false \
  npx --yes wrangler@latest deploy
