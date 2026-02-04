#!/bin/bash
# Install git hooks for rsws project
# Run this script once after cloning: ./scripts/install-hooks.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
HOOK_DIR="$PROJECT_ROOT/.git/hooks"

mkdir -p "$HOOK_DIR"

cat > "$HOOK_DIR/pre-commit" << 'HOOK'
#!/bin/bash
set -e

echo "🔍 Running pre-commit checks..."

echo "📝 Checking formatting..."
if ! cargo fmt --check; then
    echo "❌ Format check failed. Run 'cargo fmt' to fix."
    exit 1
fi

echo "🔧 Running clippy..."
if ! cargo clippy --all-features -- -D warnings; then
    echo "❌ Clippy found issues."
    exit 1
fi

echo "🧪 Running tests..."
if ! cargo test --all-features --lib 2>/dev/null; then
    echo "❌ Tests failed."
    exit 1
fi

echo "✅ All pre-commit checks passed!"
HOOK

chmod +x "$HOOK_DIR/pre-commit"
echo "✅ Git pre-commit hook installed successfully!"
echo "   The hook will run: cargo fmt --check, cargo clippy, cargo test"
