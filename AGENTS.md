# herdr.nvim

Neovim integration for [Herdr](https://herdr.dev).

## Project Purpose
Allows seamless tmux-style window navigation between Neovim windows and Herdr panes using standard Ctrl+h/j/k/l bindings.

## Stack & Architecture
- **Rust helper (`herdr-navigator`)**: A standalone Rust binary compiled from `src/main.rs`. Implements CLI commands (`register`, `release`, `focus`, `split`, `dispatch`) to query, record, and dispatch navigation to Herdr panes.
- **Neovim Plugin**: Written in Lua, located in `lua/herdr/` and `plugin/`. It manages Neovim window movement, registers/releases current Neovim pane presence cache in `~/.cache/herdr.nvim/` (using autocommands), and defers to the Rust helper if Neovim cannot navigate inside itself.

## Setup, Build & Run Commands
- Build Rust helper: `cargo build --release`
- Helper output location: `target/release/herdr-navigator`
- Neovim configuration example (Lazy):
  ```lua
  {
    "devxplay/herdr.nvim",
    build = "cargo build --release",
    config = function()
      require("herdr").setup()
    end,
  }
  ```

## Development & Verification Steps
- Check Rust build/types: `cargo check`
- Format Rust code: `cargo fmt --all`
- Lint Rust code: `cargo clippy --all-targets`
- Pre-commit Hook Installation:
  Ensure `pre-commit` is installed globally or in your environment, then run:
  ```sh
  pre-commit install
  ```
  This will set up pre-commit hooks that check trailing whitespace, YAML syntax, Cargo check, Rust formatting, StyLua formatting, and Rust clippy on commit. To run all checks manually, use:
  ```sh
  pre-commit run --all-files
  ```

## Project Conventions & Quirks
- Neovim pane registrations are temporary. The helper leverages Herdr's agent focus API internally as a temporary focus shim, then releases that marker immediately.
- The helper stores layouts and registers in local `~/.cache/herdr.nvim/` cache to avoid waiting for Herdr's debounced JSON saves.
- Prevent bracketed paste issue by utilizing `pane.send_text` in the helper.
