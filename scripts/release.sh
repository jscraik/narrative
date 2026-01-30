#!/bin/bash
# Release helper script for Narrative MVP
# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.2.1

set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Error: Version argument required"
    echo "Usage: ./scripts/release.sh <version>"
    echo "Example: ./scripts/release.sh 0.2.1"
    exit 1
fi

# Validate version format (semver)
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Error: Version must be in semver format (e.g., 0.2.1)"
    exit 1
fi

echo "=== Narrative MVP Release Script ==="
echo "Version: $VERSION"
echo ""

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo "Error: You have uncommitted changes. Please commit or stash them first."
    exit 1
fi

# Update version in package.json
echo "1. Updating package.json..."
CURRENT_VERSION=$(cat package.json | grep '"version"' | head -1 | sed 's/.*: "\(.*\)".*/\1/')
if [ "$CURRENT_VERSION" != "$VERSION" ]; then
    npm version "$VERSION" --no-git-tag-version
else
    echo "   Version already at $VERSION, skipping npm version"
fi

# Update version in tauri.conf.json
echo "2. Updating tauri.conf.json..."
sed -i.bak "s/\"version\": \"[0-9]\+\.[0-9]\+\.[0-9]\+\"/\"version\": \"$VERSION\"/" src-tauri/tauri.conf.json
rm src-tauri/tauri.conf.json.bak

# Commit version bump
echo "3. Committing version bump..."
git add package.json src-tauri/tauri.conf.json
git commit -m "chore(release): bump version to $VERSION"

# Create and push tag
echo "4. Creating git tag v$VERSION..."
git tag -a "v$VERSION" -m "Release v$VERSION"

echo "5. Pushing to origin..."
git push origin main
git push origin "v$VERSION"

echo ""
echo "=== Release triggered! ==="
echo "The GitHub Action will now build and publish the release."
echo "Monitor progress at: https://github.com/jscraik/narrative/actions"
echo ""
echo "Once complete, the release will be available at:"
echo "https://github.com/jscraik/narrative/releases/tag/v$VERSION"
