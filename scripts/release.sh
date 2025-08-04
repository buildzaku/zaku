#!/bin/bash
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RESET='\033[0m'

is_resp_yes()
{
  local prompt="$1"
  echo -e -n "$prompt"
  read -n 1 -s -r response
  echo " $response"

  [[ "$response" =~ ^[Yy]$ ]]
}

CURRENT_BRANCH=$(git branch --show-current)

if [[ "$CURRENT_BRANCH" != "main" ]]; then
  echo -e "${YELLOW}Currently on branch: $CURRENT_BRANCH${RESET}"
  if is_resp_yes "${YELLOW}Switch to main branch? (y/N)${RESET}"; then
    git checkout main
  else
    echo -e "${RED}Release aborted. Must be on main branch.${RESET}"
    exit 1
  fi
fi

if [[ -n $(git status --porcelain) ]]; then
  echo -e "${RED}You have uncommitted changes. Please commit or stash them first.${RESET}"
  exit 1
fi

echo -e "${BLUE}Pulling latest changes from origin/main...${RESET}"
if ! git pull origin main; then
  echo -e "${RED}Failed to pull latest changes.${RESET}"
  exit 1
fi

if [[ -n $(git status --porcelain) ]]; then
  echo -e "${RED}Working directory has changes after pull:${RESET}"
  git status --short
  echo -e "${YELLOW}Please resolve conflicts or stash changes before continuing.${RESET}"
  exit 1
fi

echo -e "${GREEN}Working directory is clean and up to date with origin/main${RESET}"

CURRENT_VERSION=$(git describe --tags --abbrev=0 2> /dev/null || echo "0.0.0")
echo -e "Current version: ${GREEN}$CURRENT_VERSION${RESET}"

NEXT_VERSION=$(git cliff --bumped-version --unreleased 2> /dev/null || echo "")

if [ -z "$NEXT_VERSION" ]; then
  echo -e "${YELLOW}No version bump needed based on commits since last release${RESET}"
  exit 0
fi

echo -e "Next version: ${GREEN}$NEXT_VERSION${RESET}"

echo -e "\n${BLUE}Commits since last release:${RESET}"
git --no-pager log --oneline "${CURRENT_VERSION}..HEAD" --pretty=format:"  %C(yellow)%h%C(reset) %s"

echo -e "\n\n${BLUE}Changelog preview for this release:${RESET}"
git cliff --unreleased --tag "$NEXT_VERSION" --strip all

echo -e "\n${BLUE}Files that will be updated:${RESET}"
echo "  - package.json (version bump)"
echo "  - src-tauri/Cargo.toml (via sync-metadata)"
echo "  - src-tauri/Cargo.lock (via sync-metadata)"
echo "  - snapcraft.yaml (via sync-metadata)"

echo -e "\n${BLUE}Git operations that will be performed:${RESET}"
echo -e "  1. Bump version in package.json"
echo -e "  2. Sync metadata across the repository"
echo -e "  3. Create commit: ${GREEN}release: Zaku $NEXT_VERSION${RESET}"
echo -e "  4. Create tag: ${GREEN}$NEXT_VERSION${RESET}"

if ! is_resp_yes "\n${YELLOW}Do you want to proceed with these changes? (y/N)${RESET}"; then
  echo -e "${RED}Aborted by user${RESET}"
  exit 0
fi

echo -e "\n${GREEN}Proceeding with version bump...${RESET}"

echo -e "${BLUE}Updating package.json...${RESET}"
jq ".version = \"$NEXT_VERSION\"" package.json > package.json.tmp && mv package.json.tmp package.json

echo -e "${BLUE}Syncing metadata...${RESET}"
pnpm sync-metadata

echo -e "${BLUE}Creating release commit...${RESET}"

EXPECTED_FILES=("package.json" "src-tauri/Cargo.toml" "src-tauri/Cargo.lock" "snapcraft.yaml")
MODIFIED_FILES=$(git diff --name-only)

UNEXPECTED_FILES=()
while IFS= read -r file; do
  if [ -n "$file" ]; then
    found=false
    for expected in "${EXPECTED_FILES[@]}"; do
      if [[ "$file" == "$expected" ]]; then
        found=true
        break
      fi
    done
    if [[ "$found" == false ]]; then
      UNEXPECTED_FILES+=("$file")
    fi
  fi
done <<< "$MODIFIED_FILES"

if [[ ${#UNEXPECTED_FILES[@]} -gt 0 ]]; then
  echo -e "${RED}Unexpected files were modified:${RESET}"
  printf '  %s\n' "${UNEXPECTED_FILES[@]}"
  echo -e "${RED}Please review these changes. Aborting.${RESET}"
  exit 1
fi

echo -e "${BLUE}Formatting files...${RESET}"
pnpm format > /dev/null 2>&1

git add "${EXPECTED_FILES[@]}"

echo -e "\n${BLUE}Files ready to commit:${RESET}"
git diff --cached --name-only | sed 's/^/  /'

echo -e "\n${BLUE}Changes summary:${RESET}"
echo -e "  Commit: ${GREEN}release: Zaku $NEXT_VERSION${RESET}"
echo -e "  Tag: ${GREEN}$NEXT_VERSION${RESET}"

if ! is_resp_yes "\n${YELLOW}Ready to create release. Continue? (y/N)${RESET}"; then
  echo -e "${RED}Release aborted${RESET}"
  git reset HEAD
  exit 0
fi

git commit -m "release: Zaku $NEXT_VERSION"

RELEASE_COMMIT=$(git rev-parse --short HEAD)

echo -e "${BLUE}Creating tag...${RESET}"
git tag "$NEXT_VERSION" "$RELEASE_COMMIT"
echo -e "${GREEN}Tagged commit $RELEASE_COMMIT as $NEXT_VERSION${RESET}"

echo -e "\n${GREEN}Successfully created release commit and tag $NEXT_VERSION${RESET}"

if ! is_resp_yes "\n${YELLOW}Push to remote now? (y/N)${RESET}"; then
  echo -e "${YELLOW}To push later, run:${RESET}"
  echo -e "  git push origin main"
  echo -e "  git push origin $NEXT_VERSION"
  exit 0
fi

echo -e "\n${BLUE}Pushing to remote...${RESET}"
echo -e "${BLUE}Pushing commit $RELEASE_COMMIT to main${RESET}"
if ! git push origin main; then
  echo -e "${RED}Failed to push main branch${RESET}"
  exit 1
fi

echo -e "${BLUE}Pushing tag $NEXT_VERSION${RESET}"
if ! git push origin "$NEXT_VERSION"; then
  echo -e "${RED}Failed to push tag${RESET}"
  exit 1
fi

echo -e "\n${GREEN}Release $NEXT_VERSION successfully pushed to remote!${RESET}"
