# Build environment for Raster CRT. Usage:  source scripts/env.sh
# Uses a project-local toolchain in .build/ if one exists, otherwise the Rust already
# installed on this Mac (from https://rustup.rs).
if [ -z "${RC_ROOT:-}" ]; then
  if [ -n "${BASH_SOURCE[0]:-}" ]; then _rc_env="${BASH_SOURCE[0]}"; else _rc_env="$0"; fi
  RC_ROOT="$(cd "$(dirname "$_rc_env")/.." && pwd)"
fi
RC_BUILD="$RC_ROOT/.build"

if [ -d "$RC_BUILD/rustup" ]; then
  export RUSTUP_HOME="$RC_BUILD/rustup"
  export CARGO_HOME="$RC_BUILD/cargo"
  export CARGO_TARGET_DIR="$RC_BUILD/target"
  export PATH="/opt/homebrew/opt/rustup/bin:$CARGO_HOME/bin:$PATH"
else
  export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"
  if ! command -v cargo >/dev/null 2>&1 || ! command -v just >/dev/null 2>&1; then
    echo "Raster CRT needs Rust and just. Install Rust from https://rustup.rs," >&2
    echo "then run: cargo install just" >&2
    return 1 2>/dev/null || exit 1
  fi
fi
