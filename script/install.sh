#!/usr/bin/env sh

set -eu

main() {
  version="latest"
  case "${1:-}" in
  "")
    ;;
  -h | --help)
    if [ "$#" -ne 1 ]; then
      echo "Unexpected argument: $2" >&2
      echo "Usage: ${0##*/} [OPTIONS]" >&2
      exit 1
    fi
    echo "Usage: ${0##*/} [OPTIONS]"
    echo "Install Zaku on Linux."
    echo "Options:"
    echo "  --version <version>  [default: latest]"
    echo "  -h, --help           Show help."
    exit 0
    ;;
  --version)
    if [ "$#" -ne 2 ] || [ -z "$2" ]; then
      echo "Usage: ${0##*/} [OPTIONS]" >&2
      exit 1
    fi
    version="$2"
    ;;
  *)
    echo "Unexpected argument: $1" >&2
    echo "Usage: ${0##*/} [OPTIONS]" >&2
    exit 1
    ;;
  esac

  if [ "${1:-}" = "--version" ] && [ -n "${ZAKU_BUNDLE_PATH:-}" ]; then
    echo "Cannot use --version with ZAKU_BUNDLE_PATH" >&2
    exit 1
  fi

  if [ "$(uname -s)" != "Linux" ]; then
    echo "Zaku can only be installed on Linux" >&2
    exit 1
  fi

  machine=$(uname -m)
  case "$machine" in
  aarch64 | arm64)
    arch="aarch64"
    ;;
  x86_64 | amd64)
    arch="x86_64"
    ;;
  *)
    echo "Unsupported Linux architecture: $machine" >&2
    exit 1
    ;;
  esac

  for command in mktemp sed tar; do
    if ! command -v "$command" >/dev/null 2>&1; then
      echo "Missing required command: $command" >&2
      exit 1
    fi
  done

  if [ -z "${ZAKU_BUNDLE_PATH:-}" ] && ! command -v curl >/dev/null 2>&1; then
    echo "Missing required command: curl" >&2
    exit 1
  fi
  if [ -z "${HOME:-}" ]; then
    echo "HOME is not set" >&2
    exit 1
  fi

  install_prefix="$HOME/.local"
  application_path="$install_prefix/zaku.app"
  binary_directory="$install_prefix/bin"
  binary_path="$binary_directory/zaku"
  case "${XDG_DATA_HOME:-}" in
  /*)
    desktop_directory="$XDG_DATA_HOME/applications"
    ;;
  *)
    desktop_directory="$install_prefix/share/applications"
    ;;
  esac
  desktop_path="$desktop_directory/dev.zaku.Zaku.desktop"

  mkdir -p "$install_prefix" "$binary_directory" "$desktop_directory"

  temporary_directory=""
  transaction_directory=""
  application_installed=0
  binary_installed=0
  desktop_installed=0
  has_application_backup=0
  has_binary_backup=0
  has_desktop_backup=0
  committed=0
  rollback_failed=0

  cleanup() {
    status=$?
    set +e

    if [ "$committed" -eq 0 ]; then
      if [ "$desktop_installed" -eq 1 ]; then
        if ! rm -f "$desktop_path"; then
          echo "Could not remove desktop entry during rollback: $desktop_path" >&2
          rollback_failed=1
        fi
      fi
      if [ "$has_desktop_backup" -eq 1 ]; then
        if [ -e "$desktop_backup" ] || [ -L "$desktop_backup" ]; then
          if ! mv "$desktop_backup" "$desktop_path"; then
            echo "Could not restore desktop entry: $desktop_backup" >&2
            rollback_failed=1
          fi
        elif [ ! -e "$desktop_path" ] && [ ! -L "$desktop_path" ]; then
          echo "Missing desktop entry backup: $desktop_backup" >&2
          rollback_failed=1
        fi
      fi
      if [ "$binary_installed" -eq 1 ]; then
        if ! rm -f "$binary_path"; then
          echo "Could not remove binary during rollback: $binary_path" >&2
          rollback_failed=1
        fi
      fi
      if [ "$has_binary_backup" -eq 1 ]; then
        if [ -e "$binary_backup" ] || [ -L "$binary_backup" ]; then
          if ! mv "$binary_backup" "$binary_path"; then
            echo "Could not restore binary: $binary_backup" >&2
            rollback_failed=1
          fi
        elif [ ! -e "$binary_path" ] && [ ! -L "$binary_path" ]; then
          echo "Missing binary backup: $binary_backup" >&2
          rollback_failed=1
        fi
      fi
      if [ "$application_installed" -eq 1 ]; then
        if ! rm -rf "$application_path"; then
          echo "Could not remove application during rollback: $application_path" >&2
          rollback_failed=1
        fi
      fi
      if [ "$has_application_backup" -eq 1 ]; then
        if [ -e "$application_backup" ] || [ -L "$application_backup" ]; then
          if ! mv "$application_backup" "$application_path"; then
            echo "Could not restore application: $application_backup" >&2
            rollback_failed=1
          fi
        elif [ ! -e "$application_path" ] && [ ! -L "$application_path" ]; then
          echo "Missing application backup: $application_backup" >&2
          rollback_failed=1
        fi
      fi
    fi

    if [ -n "$temporary_directory" ]; then
      if ! rm -rf "$temporary_directory"; then
        echo "Could not remove temporary directory: $temporary_directory" >&2
      fi
    fi
    if [ -n "$transaction_directory" ]; then
      if [ "$rollback_failed" -eq 1 ]; then
        echo "Rollback incomplete; backups preserved: $transaction_directory" >&2
      elif ! rm -rf "$transaction_directory"; then
        echo "Could not remove transaction directory: $transaction_directory" >&2
      fi
    fi
    return "$status"
  }

  trap cleanup 0
  trap 'exit 1' HUP INT TERM

  if [ -n "${TMPDIR:-}" ] && [ -d "$TMPDIR" ]; then
    temporary_directory=$(mktemp -d "$TMPDIR/zaku-XXXXXX")
  else
    temporary_directory=$(mktemp -d "/tmp/zaku-XXXXXX")
  fi
  transaction_directory=$(mktemp -d "$install_prefix/.zaku-install.XXXXXX")
  archive_path="$temporary_directory/zaku-linux-$arch.tar.gz"
  application_backup="$transaction_directory/zaku.previous.app"
  binary_backup="$transaction_directory/zaku.previous"
  desktop_backup="$transaction_directory/dev.zaku.Zaku.previous.desktop"

  if [ -n "${ZAKU_BUNDLE_PATH:-}" ]; then
    if [ ! -f "$ZAKU_BUNDLE_PATH" ]; then
      echo "Zaku bundle does not exist: $ZAKU_BUNDLE_PATH" >&2
      exit 1
    fi
    cp "$ZAKU_BUNDLE_PATH" "$archive_path"
  else
    echo "Downloading Zaku $version"
    curl -fL "https://api.zaku.dev/releases/stable/$version/linux-$arch/download" -o "$archive_path"
  fi

  extracted_directory="$transaction_directory/extracted"
  mkdir "$extracted_directory"
  tar -xzf "$archive_path" -C "$extracted_directory"

  staged_application="$extracted_directory/zaku.app"
  staged_binary="$staged_application/libexec/zaku"
  staged_desktop="$staged_application/share/applications/dev.zaku.Zaku.desktop"
  staged_icon="$staged_application/share/icons/hicolor/512x512/apps/zaku.png"

  if [ ! -x "$staged_binary" ]; then
    echo "Zaku bundle does not contain an executable" >&2
    exit 1
  fi
  if [ ! -f "$staged_desktop" ]; then
    echo "Zaku bundle does not contain a desktop entry" >&2
    exit 1
  fi
  if [ ! -f "$staged_icon" ]; then
    echo "Zaku bundle does not contain an application icon" >&2
    exit 1
  fi

  echo "Installing Zaku"
  prepared_desktop="$transaction_directory/dev.zaku.Zaku.desktop"
  desktop_executable=$(printf '%s' "$application_path/libexec/zaku" | sed -e 's/\\/\\\\\\\\/g' -e 's/["`$]/\\\\&/g' -e 's/%/%%/g')
  desktop_application_path=$(printf '%s' "$application_path" | sed 's/\\/\\\\/g')
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
    TryExec=*)
      printf 'TryExec=%s/libexec/zaku\n' "$desktop_application_path"
      ;;
    Exec=*)
      printf 'Exec="%s"\n' "$desktop_executable"
      ;;
    Icon=*)
      printf 'Icon=%s/share/icons/hicolor/512x512/apps/zaku.png\n' "$desktop_application_path"
      ;;
    *)
      printf '%s\n' "$line"
      ;;
    esac
  done <"$staged_desktop" >"$prepared_desktop"
  chmod +x "$prepared_desktop"

  if [ -e "$application_path" ] || [ -L "$application_path" ]; then
    has_application_backup=1
    mv "$application_path" "$application_backup"
  fi
  if [ -e "$binary_path" ] || [ -L "$binary_path" ]; then
    has_binary_backup=1
    mv "$binary_path" "$binary_backup"
  fi
  if [ -e "$desktop_path" ] || [ -L "$desktop_path" ]; then
    has_desktop_backup=1
    mv "$desktop_path" "$desktop_backup"
  fi

  application_installed=1
  mv "$staged_application" "$application_path"
  binary_installed=1
  ln -s "$application_path/libexec/zaku" "$binary_path"
  desktop_installed=1
  mv "$prepared_desktop" "$desktop_path"
  committed=1

  echo "Installed Zaku: $binary_directory/zaku"
  if [ "$(command -v zaku)" != "$binary_path" ]; then
    echo "Add $binary_directory to PATH to run 'zaku'"
  fi
}

main "$@"
