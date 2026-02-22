#!/bin/bash
set -e

# --- Load Environment Variables ---
# Automatically export all variables defined in .env
if [ -f .env ]; then
    set -a
    source .env
    set +a
else
    echo "⚠️  No .env file found. Using default fallbacks."
fi

PLUGIN_PATH="${PLUGIN_DIR:-saint-plugin}"
WASM_SRC_NAME="${COMPILED_WASM_NAME:-saint_plugin.wasm}"
WASM_DEST_DIR="${DEST_DIR:-plugins}"
WASM_DEST_NAME="${DEST_NAME:-logic1.wasm}"

echo "🦀 Building WASM plugin in release mode..."
cd "$PLUGIN_PATH"
cargo component build --release
cd -
echo $PWD
echo "$WASM_DEST_DIR/$WASM_DEST_NAME"

echo "📦 Moving and renaming WASM payload..."
mkdir -p "$WASM_DEST_DIR"
rm -f "$WASM_DEST_DIR/$WASM_DEST_NAME"
cp "$PLUGIN_PATH/target/wasm32-wasip1/release/$WASM_SRC_NAME" "$WASM_DEST_DIR/$WASM_DEST_NAME"

echo "✅ Success! Brain injected at $WASM_DEST_DIR/$WASM_DEST_NAME."
