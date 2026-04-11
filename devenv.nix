{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  extra = inputs.ifiokjr-nixpkgs.packages.${pkgs.stdenv.system};
in

{
  packages =
    with pkgs;
    [
      cargo-binstall
      cargo-run-bin
      deno
      dprint
      extra.pnpm-standalone
      mdbook
      nixfmt
      rustup
      shfmt
    ]
    ++ lib.optionals stdenv.isDarwin [
      coreutils
    ];

  enterShell = ''
    set -e
    # Ensure the nightly toolchain is available for rustfmt (used by dprint)
    rustup toolchain install nightly --component rustfmt --no-self-update 2>/dev/null || true
    # Ensure stable is at least 1.86 (required by edition 2024)
    rustup update stable --no-self-update 2>/dev/null || true
    eval "$(pnpm-activate-env)"
  '';

  # disable dotenv since it breaks the variable interpolation supported by `direnv`
  dotenv.disableHint = true;

  git-hooks = {
    # package = pkgs.prek;

    hooks = {
      "secrets:commit" = {
        enable = true;
        verbose = true;
        pass_filenames = true;
        name = "secrets";
        description = "Scan staged changes for leaked secrets with gitleaks.";
        entry = "${pkgs.gitleaks}/bin/gitleaks protect --staged --verbose --redact";
        stages = [ "pre-commit" ];
        extraPackages = with pkgs; [
          gitleaks
        ];
      };
      dprint = {
        enable = true;
        verbose = true;
        pass_filenames = true;
        name = "dprint check";
        description = "Run workspace autofixes before commit and restage the results.";
        entry = "${pkgs.dprint}/bin/dprint check --allow-no-files";
        stages = [ "pre-commit" ];
      };
      "secrets:push" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "secrets";
        description = "Scan repository history for leaked secrets with gitleaks before push.";
        entry = "${pkgs.gitleaks}/bin/gitleaks detect --verbose --redact";
        stages = [ "pre-push" ];
        extraPackages = with pkgs; [
          gitleaks
        ];
      };
      "lint" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "lint";
        description = "Run the local CI lint rules suite before push.";
        entry = "${config.env.DEVENV_PROFILE}/bin/lint:all";
        stages = [ "pre-push" ];
      };
      "test" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "test";
        description = "Run the local CI validation suite before push.";
        entry = "${config.env.DEVENV_PROFILE}/bin/test:all";
        stages = [ "pre-push" ];
      };
    };
  };

  scripts = {
    "install:all" = {
      exec = ''
        set -e
        install:cargo:bin
      '';
      description = "Install all packages.";
      binary = "bash";
    };
    "install:cargo:bin" = {
      exec = ''
        set -e
        cargo bin --install
      '';
      description = "Install cargo binaries locally.";
      binary = "bash";
    };
    "update:deps" = {
      exec = ''
        set -e
        cargo update
        devenv update
      '';
      description = "Update dependencies.";
      binary = "bash";
    };
    "build:all" = {
      exec = ''
        set -e
        if [ -z "$CI" ]; then
          echo "Building project locally"
          cargo build --workspace
        else
          echo "Building in CI"
          cargo build --workspace --locked
        fi
      '';
      description = "Build all crates in the workspace.";
      binary = "bash";
    };
    "build:book" = {
      exec = ''
        set -e
        mdbook build docs
      '';
      description = "Build the mdbook documentation.";
      binary = "bash";
    };
    "test:all" = {
      exec = ''
        set -e
        test:cargo
        test:docs
      '';
      description = "Run all tests across the crates.";
      binary = "bash";
    };
    "test:cargo" = {
      exec = ''
        set -e
        cargo nextest run --workspace
      '';
      description = "Run cargo tests with nextest.";
      binary = "bash";
    };
    "test:docs" = {
      exec = ''
        set -e
        cargo test --workspace --doc
      '';
      description = "Run documentation tests.";
      binary = "bash";
    };
    "coverage:all" = {
      exec = ''
        set -e
        cargo llvm-cov nextest --workspace --lcov --output-path lcov.info
      '';
      description = "Run coverage across the crates.";
      binary = "bash";
    };
    "fix:all" = {
      exec = ''
        set -e
        fix:clippy
        fix:format
      '';
      description = "Fix all autofixable problems.";
      binary = "bash";
    };
    "fix:format" = {
      exec = ''
        set -e
        dprint fmt --config "$DEVENV_ROOT/dprint.json"
      '';
      description = "Format files with dprint.";
      binary = "bash";
    };
    "fix:clippy" = {
      exec = ''
        set -e
        cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
      '';
      description = "Fix clippy lints for rust.";
      binary = "bash";
    };
    "deny:check" = {
      exec = ''
        set -e
        cargo deny check
      '';
      description = "Run cargo-deny checks for security advisories and license compliance.";
      binary = "bash";
    };
    "lint:all" = {
      exec = ''
        set -e
        lint:clippy
        lint:format
        deny:check
      '';
      description = "Run all checks.";
      binary = "bash";
    };
    "lint:format" = {
      exec = ''
        set -e
        dprint check
      '';
      description = "Check that all files are formatted.";
      binary = "bash";
    };
    "lint:clippy" = {
      exec = ''
        set -e
        cargo clippy --workspace --all-targets -- -D warnings
      '';
      description = "Check that all rust lints are passing.";
      binary = "bash";
    };
    "snapshot:review" = {
      exec = ''
        set -e
        cargo insta review
      '';
      description = "Review insta snapshots.";
      binary = "bash";
    };
    "snapshot:update" = {
      exec = ''
        set -e
        cargo nextest run --workspace
        cargo insta accept
      '';
      description = "Update insta snapshots.";
      binary = "bash";
    };
  };
}
