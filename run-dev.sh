#!/usr/bin/env bash
set -euo pipefail

# Переходимо в корінь проєкту незалежно від поточного каталогу.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

glib-compile-schemas data
cargo run -- /home/monnomax/Зображення/rfvvhhgddfvb.webp
