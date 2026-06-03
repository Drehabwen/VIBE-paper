# Task Plan

## Objective
- Improve Windows launch stability by adding a durable launcher, logging, shortcut creation, and a small CLI launch check.

## Constraints
- Preserve existing user changes in `rust/Cargo.lock` and `claw.ps1`.
- Keep changes scoped to Windows launch ergonomics and documentation.
- Prefer scripts for Windows shell integration instead of broad CLI refactors.

## Steps
- [completed] Inspect current implementation and confirm launch failure mode.
- [completed] Add Windows launcher and shortcut installation script with logs.
- [completed] Add a CLI `launch-check` command for quick diagnostics.
- [completed] Update docs/help so users can discover the stable launcher.
- [completed] Run targeted verification and record outcome.
- [completed] Add `claw logs` for opening and inspecting launcher logs.
- [completed] Add Windows install script for release build, PATH, and shortcut setup.
- [completed] Re-check README/USAGE encoding and update docs for the new commands.
- [completed] Run verification for the second phase.
- [completed] Re-read the handoff plan and tighten the new `claw logs` help/test coverage.

## Verification
- Run PowerShell script syntax checks where feasible.
- Run `cargo fmt` and targeted CLI tests/build if Rust changes are made.
- Run `claw launch-check` from the local debug binary.

## Outcome
- Added Windows launcher and shortcut installer scripts under `scripts/windows/`.
- Added `claw launch-check` with text and JSON output.
- Added `claw logs` with `--last`, `--dir`, and `--open`.
- Added `scripts/windows/Install-ClawCode.ps1` for release install, optional PATH update, and shortcut refresh.
- Updated README and USAGE with the stable Windows launcher workflow.
- Recreated `C:\Users\DORAT\Desktop\Galen.lnk`.
- Verification passed: PowerShell parser checks, `cargo build -p rusty-claude-cli`, `cargo build --release -p rusty-claude-cli`, targeted bin test for command parsing, `claw launch-check`, and `claw logs`.
- Follow-up verification passed: `cargo fmt`; `cargo test -p rusty-claude-cli --bin claw removed_login_and_logout_subcommands_error_helpfully -- --nocapture`.
- Remaining warning: user-level PATH updates may fail under current registry policy; installer now warns and continues.
- Remaining Windows test blocker: `cargo test -p rusty-claude-cli ...` still compiles `tests/mock_parity_harness.rs`, which imports `std::os::unix::fs::PermissionsExt`; this is pre-existing Windows incompatibility outside the launcher work.

## Handoff
- Current branch has unrelated pre-existing edits in `rust/Cargo.lock`, `rust/crates/api/src/providers/mod.rs`, `rust/crates/api/src/providers/openai_compat.rs`, and `claw.ps1`; do not revert them.
- Primary deliverable is already in place: stable Windows launcher, shortcut installer, install script, and logging commands.
- Best next step for a follow-on AI is to decide whether to tighten release packaging or clean the remaining repo warnings.
- If resuming work, start with `scripts/windows/Install-ClawCode.ps1`, `scripts/windows/Start-ClawCode.ps1`, and `rust/crates/rusty-claude-cli/src/main.rs`.
