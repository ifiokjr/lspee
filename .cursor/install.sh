#!/usr/bin/env bash
# Idempotent toolchain bootstrap for Cursor cloud agents.
# Installs the pinned Rust toolchain, formatters used by dprint, and the
# cargo binaries declared in [workspace.metadata.bin].
set -euo pipefail

DPRINT_VERSION="0.64.3"
DPRINT_SHA256="45c75486932b1c85c7594da353a2e063be883299d39780552147a0ab15ac7f1e"
SHFMT_VERSION="3.14.1"
SHFMT_SHA256="76e77641faa025814b77f153b29796b8e6fa2fca03e0c76a691608b86c7ea7bf"
NIXFMT_VERSION="1.5.0"
NIXFMT_SHA256="aa029247ab89c897e2f3948463967553f902f86e86e3402fa28cb29c4a44f745"
RUST_STABLE="1.88.0"
CARGO_RUN_BIN_VERSION="1.7.5"

if [[ -z "${CARGO_HOME:-}" ]]; then
	if [[ -d /usr/local/cargo ]]; then
		export CARGO_HOME=/usr/local/cargo
		export RUSTUP_HOME="${RUSTUP_HOME:-/usr/local/rustup}"
	else
		export CARGO_HOME="${HOME}/.cargo"
		export RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}"
	fi
fi
export PATH="${CARGO_HOME}/bin:${PATH}"

install_apt() {
	local missing=()
	command -v gcc >/dev/null 2>&1 || missing+=(build-essential)
	command -v pkg-config >/dev/null 2>&1 || missing+=(pkg-config)
	command -v curl >/dev/null 2>&1 || missing+=(ca-certificates curl)
	command -v unzip >/dev/null 2>&1 || missing+=(unzip)
	command -v ps >/dev/null 2>&1 || missing+=(procps)
	if ((${#missing[@]} == 0)); then
		return 0
	fi
	sudo apt-get update
	sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${missing[@]}"
}

install_rustup() {
	if command -v rustup >/dev/null 2>&1; then
		return 0
	fi
	curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs |
		sh -s -- -y --default-toolchain none --profile minimal --no-modify-path
}

write_login_path() {
	local profile=/etc/profile.d/lspee-cloud.sh
	local cargo_home="${CARGO_HOME}"
	local rustup_home="${RUSTUP_HOME}"
	sudo tee "${profile}" >/dev/null <<EOF
# Added by lspee cloud-agent install. Login shells do not source ~/.bashrc.
export CARGO_HOME="\${CARGO_HOME:-${cargo_home}}"
export RUSTUP_HOME="\${RUSTUP_HOME:-${rustup_home}}"
case ":\${PATH}:" in
	*:"\${CARGO_HOME}/bin":*) ;;
	*) export PATH="\${CARGO_HOME}/bin:\${PATH}" ;;
esac
EOF
}

verify_sha256() {
	local file="$1"
	local expected="$2"
	echo "${expected}  ${file}" | sha256sum -c -
}

install_dprint() {
	if dprint --version 2>/dev/null | grep -q "${DPRINT_VERSION}"; then
		return 0
	fi
	local tmp
	tmp="$(mktemp -d)"
	curl -fsSL -o "${tmp}/dprint.zip" \
		"https://github.com/dprint/dprint/releases/download/${DPRINT_VERSION}/dprint-x86_64-unknown-linux-gnu.zip"
	unzip -q -o "${tmp}/dprint.zip" -d "${tmp}"
	verify_sha256 "${tmp}/dprint" "${DPRINT_SHA256}"
	sudo install -m 0755 "${tmp}/dprint" /usr/local/bin/dprint
	rm -rf "${tmp}"
}

install_shfmt() {
	if shfmt --version 2>/dev/null | grep -q "v${SHFMT_VERSION}"; then
		return 0
	fi
	local tmp
	tmp="$(mktemp)"
	curl -fsSL -o "${tmp}" \
		"https://github.com/mvdan/sh/releases/download/v${SHFMT_VERSION}/shfmt_v${SHFMT_VERSION}_linux_amd64"
	verify_sha256 "${tmp}" "${SHFMT_SHA256}"
	sudo install -m 0755 "${tmp}" /usr/local/bin/shfmt
	rm -f "${tmp}"
}

install_nixfmt() {
	if nixfmt --version 2>/dev/null | grep -q "${NIXFMT_VERSION}"; then
		return 0
	fi
	local tmp
	tmp="$(mktemp)"
	curl -fsSL -o "${tmp}" \
		"https://github.com/NixOS/nixfmt/releases/download/v${NIXFMT_VERSION}/nixfmt"
	verify_sha256 "${tmp}" "${NIXFMT_SHA256}"
	sudo install -m 0755 "${tmp}" /usr/local/bin/nixfmt
	rm -f "${tmp}"
}

install_cargo_tools() {
	if ! command -v cargo-binstall >/dev/null 2>&1; then
		curl -L --proto '=https' --tlsv1.2 -fsSL \
			https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh |
			bash
	fi
	if ! cargo bin --version >/dev/null 2>&1; then
		cargo binstall --no-confirm "cargo-run-bin@${CARGO_RUN_BIN_VERSION}"
	fi
	cargo bin --install
	cargo fetch --locked
}

install_apt
install_rustup
write_login_path
rustup toolchain install "${RUST_STABLE}" --profile minimal \
	--component rustfmt,clippy,rust-analyzer --no-self-update
rustup toolchain install nightly --profile minimal --component rustfmt --no-self-update
install_dprint
install_shfmt
install_nixfmt
install_cargo_tools
echo "lspee cloud install complete"
rustc --version
cargo --version
dprint --version
shfmt --version
nixfmt --version
cargo bin --version
