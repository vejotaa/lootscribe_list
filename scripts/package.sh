#!/usr/bin/env bash
set -euo pipefail

# Garante execução a partir da raiz do repositório, independente de onde o script for chamado
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

APP_NAME="lootscribe_list"
VERSION=$(grep -m1 '^version' Cargo.toml | sed -E 's/version = "(.*)"/\1/')
DIST_DIR="dist"

LINUX_TARGET="x86_64-unknown-linux-gnu"
WIN_TARGET="x86_64-pc-windows-gnu"

echo "==> Limpando diretório ${DIST_DIR}/..."
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

echo "==> Rodando testes unitários..."
cargo test --quiet

echo "==> Compilando release Linux (${LINUX_TARGET})..."
cargo build --release --target "${LINUX_TARGET}"

echo "==> Compilando release Windows (${WIN_TARGET})..."
cargo build --release --target "${WIN_TARGET}"

# --- Empacotamento Linux (.tar.gz) ---
LINUX_PKG="${APP_NAME}-v${VERSION}-${LINUX_TARGET}"
mkdir -p "${DIST_DIR}/${LINUX_PKG}"
cp "target/${LINUX_TARGET}/release/${APP_NAME}" "${DIST_DIR}/${LINUX_PKG}/"
cp README.md "${DIST_DIR}/${LINUX_PKG}/"
cp .env.example "${DIST_DIR}/${LINUX_PKG}/"

tar -czf "${DIST_DIR}/${LINUX_PKG}.tar.gz" -C "${DIST_DIR}" "${LINUX_PKG}"
rm -rf "${DIST_DIR}/${LINUX_PKG}"

# --- Empacotamento Windows (.zip) ---
WIN_PKG="${APP_NAME}-v${VERSION}-${WIN_TARGET}"
mkdir -p "${DIST_DIR}/${WIN_PKG}"
cp "target/${WIN_TARGET}/release/${APP_NAME}.exe" "${DIST_DIR}/${WIN_PKG}/"
cp README.md "${DIST_DIR}/${WIN_PKG}/"
cp .env.example "${DIST_DIR}/${WIN_PKG}/"

(cd "${DIST_DIR}" && zip -rq "${WIN_PKG}.zip" "${WIN_PKG}")
rm -rf "${DIST_DIR}/${WIN_PKG}"

echo "==> Gerando checksums SHA256..."
(cd "${DIST_DIR}" && sha256sum *.tar.gz *.zip > SHA256SUMS.txt)

echo "==> Concluído! Artefatos gerados em ${DIST_DIR}/:"
ls -lh "${DIST_DIR}"
