#!/usr/bin/env bash
# Idempotent toolchain bootstrap for Cursor cloud agents.
# Installs single-user Nix (flakes + nix-command), devenv with the public
# devenv and nix-community binary caches, then the pinned Rust toolchain,
# dprint formatters, and cargo binaries from [workspace.metadata.bin].
# Single-user Nix is used because cloud VMs do not run systemd, so a
# multi-user nix-daemon would not stay up.
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
	command -v xz >/dev/null 2>&1 || missing+=(xz-utils)
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

source_nix() {
	if [[ -f "${HOME}/.nix-profile/etc/profile.d/nix.sh" ]]; then
		# shellcheck disable=SC1091
		. "${HOME}/.nix-profile/etc/profile.d/nix.sh"
	elif [[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]]; then
		# shellcheck disable=SC1091
		. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
	fi
	if [[ -d "${HOME}/.nix-profile/bin" ]]; then
		case ":${PATH}:" in
		*":${HOME}/.nix-profile/bin:"*) ;;
		*) export PATH="${HOME}/.nix-profile/bin:${PATH}" ;;
		esac
	fi
}

write_nix_conf_block() {
	local dest="$1"
	local use_sudo="${2:-}"
	local runner=()
	if [[ "${use_sudo}" == "sudo" ]]; then
		runner=(sudo)
	fi
	if [[ -f "${dest}" ]] && grep -q 'lspee-cloud-nix' "${dest}"; then
		return 0
	fi
	"${runner[@]}" mkdir -p "$(dirname "${dest}")"
	"${runner[@]}" tee -a "${dest}" >/dev/null <<'EOF'
# lspee-cloud-nix
experimental-features = nix-command flakes
# lspee-cloud-nix-end
EOF
}

# Nix 2.35 reads `substituters`, not `extra-substituters`. `cachix use`
# rewrites that line, so append the public caches after it runs.
ensure_binary_caches() {
	local file="${HOME}/.config/nix/nix.conf"
	local community_url="https://nix-community.cachix.org"
	local community_key="nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
	local devenv_url="https://devenv.cachix.org"
	local devenv_key="devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
	mkdir -p "${HOME}/.config/nix"
	touch "${file}"
	if ! grep -q "^substituters = .*devenv.cachix.org" "${file}"; then
		if grep -q "^substituters = " "${file}"; then
			sed -i "/^substituters = / s|$| ${devenv_url}|" "${file}"
		else
			echo "substituters = https://cache.nixos.org ${devenv_url}" >>"${file}"
		fi
	fi
	if ! grep -q "^substituters = .*nix-community.cachix.org" "${file}"; then
		sed -i "/^substituters = / s|$| ${community_url}|" "${file}"
	fi
	if ! grep -q "^trusted-public-keys = .*devenv.cachix.org-1:" "${file}"; then
		if grep -q "^trusted-public-keys = " "${file}"; then
			sed -i "/^trusted-public-keys = / s|$| ${devenv_key}|" "${file}"
		else
			echo "trusted-public-keys = ${devenv_key}" >>"${file}"
		fi
	fi
	if ! grep -q "^trusted-public-keys = .*nix-community.cachix.org-1:" "${file}"; then
		sed -i "/^trusted-public-keys = / s|$| ${community_key}|" "${file}"
	fi
}

install_nix() {
	if [[ ! -x "${HOME}/.nix-profile/bin/nix" && ! -x /nix/var/nix/profiles/default/bin/nix ]]; then
		curl -fsSL https://nixos.org/nix/install | sh -s -- --no-daemon
	fi
	source_nix
	command -v nix >/dev/null 2>&1
	mkdir -p "${HOME}/.config/nix"
	write_nix_conf_block "${HOME}/.config/nix/nix.conf"
	write_nix_conf_block /etc/nix/nix.conf sudo
}

write_nix_shell_path() {
	sudo tee /etc/profile.d/lspee-nix.sh >/dev/null <<'EOF'
# lspee-cloud-nix: login shells and any shell that sources profile.d.
if [ -f "${HOME}/.nix-profile/etc/profile.d/nix.sh" ]; then
	. "${HOME}/.nix-profile/etc/profile.d/nix.sh"
fi
if [ -d "${HOME}/.nix-profile/bin" ]; then
	case ":${PATH}:" in
		*":${HOME}/.nix-profile/bin:"*) ;;
		*) export PATH="${HOME}/.nix-profile/bin:${PATH}" ;;
	esac
fi
EOF
	if ! grep -q 'lspee-cloud-nix' /etc/bash.bashrc; then
		sudo tee -a /etc/bash.bashrc >/dev/null <<'EOF'
# lspee-cloud-nix: interactive non-login shells
if [ -f /etc/profile.d/lspee-nix.sh ]; then
	. /etc/profile.d/lspee-nix.sh
fi
EOF
	fi
}

link_nix_bins() {
	local bindir="${HOME}/.nix-profile/bin"
	local cmd
	for cmd in nix nix-build nix-channel nix-env nix-instantiate nix-shell nix-store devenv cachix; do
		if [[ -x "${bindir}/${cmd}" ]]; then
			sudo ln -sfn "${bindir}/${cmd}" "/usr/local/bin/${cmd}"
		fi
	done
}

nix_profile_install() {
	local pkg="$1"
	local attempt
	for attempt in 1 2 3; do
		if nix profile install --accept-flake-config "${pkg}"; then
			return 0
		fi
		if [[ "${attempt}" -eq 3 ]]; then
			echo "nix profile install ${pkg} failed" >&2
			return 1
		fi
		sleep $((attempt * 10))
	done
}

install_devenv() {
	source_nix
	if ! command -v cachix >/dev/null 2>&1; then
		nix_profile_install nixpkgs#cachix
		source_nix
	fi
	cachix use devenv
	ensure_binary_caches
	if ! command -v devenv >/dev/null 2>&1; then
		nix_profile_install nixpkgs#devenv
		source_nix
	fi
	link_nix_bins
}

warm_devenv() {
	local root attempt
	root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
	cd "${root}"
	for attempt in 1 2 3; do
		if devenv shell -- true && devenv shell -- install:cargo:bin; then
			return 0
		fi
		if [[ "${attempt}" -eq 3 ]]; then
			echo "devenv shell failed after ${attempt} attempts" >&2
			return 1
		fi
		echo "devenv shell failed (attempt ${attempt}); retrying" >&2
		sleep $((attempt * 10))
	done
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
install_nix
write_nix_shell_path
install_devenv
warm_devenv
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
nix --version
devenv version
rustc --version
cargo --version
dprint --version
shfmt --version
nixfmt --version
cargo bin --version
