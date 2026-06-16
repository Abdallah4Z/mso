#!/bin/sh
set -eu

REPO="Abdallah4Z/mso"
BRANCH="master"
ARCH=$(uname -m)

case "$ARCH" in
  x86_64)  ARCH="x86_64-unknown-linux-gnu" ;;
  aarch64) ARCH="aarch64-unknown-linux-gnu" ;;
  armv7l)  ARCH="armv7-unknown-linux-gnueabihf" ;;
  *)       echo "unsupported architecture: $ARCH"; exit 1 ;;
esac

if command -v dpkg >/dev/null 2>&1; then
  echo "installing mso via dpkg..."
  curl -fsSL "https://github.com/$REPO/releases/latest/download/mso-$ARCH.deb" -o /tmp/mso.deb
  sudo dpkg -i /tmp/mso.deb
  rm /tmp/mso.deb
elif command -v rpm >/dev/null 2>&1; then
  echo "installing mso via rpm..."
  curl -fsSL "https://github.com/$REPO/releases/latest/download/mso-$ARCH.rpm" -o /tmp/mso.rpm
  sudo rpm -i /tmp/mso.rpm
  rm /tmp/mso.rpm
else
  echo "installing mso binary to /usr/local/bin..."
  sudo curl -fsSL "https://github.com/$REPO/releases/latest/download/mso-$ARCH" -o /usr/local/bin/mso
  sudo chmod +x /usr/local/bin/mso
fi

echo "mso installed successfully"
mso --help
