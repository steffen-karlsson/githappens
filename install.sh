#!/usr/bin/env bash
set -euo pipefail

APP=githappens
REPO=steffen-karlsson/githappens
INSTALL_DIR="$HOME/.${APP}/bin"

MUTED='\033[0;2m'
RED='\033[0;31m'
ORANGE='\033[0;33m'
GREEN='\033[0;32m'
NC='\033[0m'

usage() {
    cat <<EOF
githappens installer

Usage: install.sh [options]

Options:
    -h, --help              Display this help message
    -v, --version <version> Install a specific version (e.g., 0.1.0)
        --no-modify-path    Don't modify shell config files (.zshrc, .bashrc, etc.)

Examples:
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash -s -- --version 0.1.0
EOF
}

requested_version=""
no_modify_path=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        -v|--version)
            if [[ -n "${2:-}" ]]; then
                requested_version="$2"
                shift 2
            else
                echo -e "${RED}Error: --version requires a version argument${NC}"
                exit 1
            fi
            ;;
        --no-modify-path)
            no_modify_path=true
            shift
            ;;
        *)
            echo -e "${RED}Error: Unknown option '$1'${NC}" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$INSTALL_DIR"

# Detect OS
raw_os=$(uname -s)
case "$raw_os" in
    Darwin*) os="darwin" ;;
    Linux*)  os="linux" ;;
    MINGW*|MSYS*|CYGWIN*) os="windows" ;;
    *) echo -e "${RED}Unsupported OS: $raw_os${NC}"; exit 1 ;;
esac

# Detect architecture
arch=$(uname -m)
case "$arch" in
    x86_64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) echo -e "${RED}Unsupported architecture: $arch${NC}"; exit 1 ;;
esac

# Map to release target triple
case "$os-$arch" in
    linux-x86_64)   target="x86_64-unknown-linux-gnu" ;;
    linux-aarch64)  target="aarch64-unknown-linux-gnu" ;;
    darwin-aarch64) target="aarch64-apple-darwin" ;;
    darwin-x86_64)  target="x86_64-apple-darwin" ;;
    windows-x86_64) target="x86_64-pc-windows-msvc" ;;
    *) echo -e "${RED}Unsupported platform: $os/$arch${NC}"; exit 1 ;;
esac

archive_ext=".tar.gz"
if [ "$os" = "windows" ]; then
    archive_ext=".zip"
fi

# Check dependencies before any network calls
if ! command -v curl >/dev/null 2>&1; then
    echo -e "${RED}Error: 'curl' is required but not installed.${NC}"
    exit 1
fi

if [ "$os" = "linux" ] || [ "$os" = "darwin" ]; then
    if ! command -v tar >/dev/null 2>&1; then
        echo -e "${RED}Error: 'tar' is required but not installed.${NC}"
        exit 1
    fi
fi

if [ "$os" = "windows" ]; then
    if ! command -v unzip >/dev/null 2>&1; then
        echo -e "${RED}Error: 'unzip' is required but not installed.${NC}"
        exit 1
    fi
fi

# Resolve version and download URL
if [ -z "$requested_version" ]; then
    version=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p')
    if [ -z "$version" ]; then
        echo -e "${RED}Failed to fetch latest version${NC}"
        exit 1
    fi
else
    requested_version="${requested_version#v}"
    version="$requested_version"
fi

url="https://github.com/${REPO}/releases/download/v${version}/${APP}-${version}-${target}${archive_ext}"
filename="${APP}-${version}-${target}${archive_ext}"

echo ""
echo -e "${MUTED}Installing ${NC}${APP} ${MUTED}version: ${NC}v${version}"
echo -e "${MUTED}Platform: ${NC}${os}/${arch} ${MUTED}→ ${NC}${target}"

# Download
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

echo -e "${MUTED}Downloading: ${NC}${url}"
curl -fsSL -o "$tmp_dir/$filename" "$url"

# Extract
if [ "$os" = "windows" ]; then
    unzip -q "$tmp_dir/$filename" -d "$tmp_dir"
else
    tar -xzf "$tmp_dir/$filename" -C "$tmp_dir"
fi

# Install binary
binary="${APP}"
if [ "$os" = "windows" ]; then
    binary="${APP}.exe"
fi

mv "$tmp_dir/$binary" "$INSTALL_DIR/"
chmod 755 "$INSTALL_DIR/$binary"
rm -f "$tmp_dir/$filename"

echo -e "${GREEN}Installed${NC} ${APP} v${version} ${MUTED}→ ${NC}$INSTALL_DIR/$binary"

# Check if already on PATH
if [[ ":$PATH:" == *":$INSTALL_DIR:"* ]]; then
    echo -e "${MUTED}Already on \$PATH${NC}"
else
    # Add to shell config
    if [[ "$no_modify_path" != "true" ]]; then
        current_shell=$(basename "${SHELL:-sh}")
        case "$current_shell" in
            fish)
                config_file="$HOME/.config/fish/config.fish"
                path_cmd="fish_add_path $INSTALL_DIR"
                ;;
            zsh)
                config_file="${ZDOTDIR:-$HOME}/.zshrc"
                path_cmd="export PATH=\"$INSTALL_DIR:\$PATH\""
                ;;
            bash)
                config_file="$HOME/.bashrc"
                path_cmd="export PATH=\"$INSTALL_DIR:\$PATH\""
                ;;
            *)
                config_file="$HOME/.profile"
                path_cmd="export PATH=\"$INSTALL_DIR:\$PATH\""
                ;;
        esac

        mkdir -p "$(dirname "$config_file")"

        if grep -Fxq "$path_cmd" "$config_file" 2>/dev/null; then
            echo -e "${MUTED}PATH entry already exists in ${NC}$config_file"
        elif [ -w "$config_file" ] || [ ! -f "$config_file" ]; then
            echo "" >> "$config_file"
            echo "# githappens" >> "$config_file"
            echo "$path_cmd" >> "$config_file"
            echo -e "${GREEN}Added ${NC}$INSTALL_DIR ${MUTED}to \$PATH in ${NC}$config_file"
        else
            echo -e "${ORANGE}Couldn't write to ${NC}$config_file${ORANGE}. Add manually:${NC}"
            echo -e "  $path_cmd"
        fi
    fi
fi

# GitHub Actions support
if [ -n "${GITHUB_ACTIONS-}" ] && [ "${GITHUB_ACTIONS}" == "true" ]; then
    echo "$INSTALL_DIR" >> "$GITHUB_PATH"
    echo -e "${MUTED}Added ${NC}$INSTALL_DIR ${MUTED}to \$GITHUB_PATH${NC}"
fi

echo ""
echo -e "${MUTED}Git happens. Now you can see it.${NC}"
echo ""
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]] && [[ "$no_modify_path" != "true" ]]; then
    echo -e "${ORANGE}Note:${NC} \$PATH updated for new shells only."
    echo -e "${MUTED}Run this now, or open a new terminal:${NC}"
    current_shell=$(basename "${SHELL:-sh}")
    case "$current_shell" in
        fish)
            echo -e "  fish_add_path $INSTALL_DIR"
            ;;
        *)
            echo -e "  export PATH=\"$INSTALL_DIR:\$PATH\""
            ;;
    esac
    echo ""
fi
echo -e "${MUTED}To get started:${NC}"
echo -e "  export GIT_TOKEN=<your-github-token>"
echo -e "  ${APP}"
echo ""
