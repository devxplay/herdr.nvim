# herdr.nvim

Neovim integration for [Herdr](https://herdr.dev).

The first feature is tmux-style navigation between Neovim windows and Herdr panes with one key family:

- `Ctrl+h`
- `Ctrl+j`
- `Ctrl+k`
- `Ctrl+l`

## How it works

Herdr does not currently expose tmux-style process-aware key forwarding. `herdr.nvim` works around that with two pieces:

1. The Neovim plugin records the current Herdr pane in a local `~/.cache/herdr.nvim/` presence cache while Neovim is open.
2. The Rust `herdr-navigator` helper is bound in Herdr. When `Ctrl+h/j/k/l` is pressed:
   - if the active pane is recorded as a Neovim pane, it forwards the key into Neovim;
   - otherwise it moves focus to the adjacent Herdr pane.

When Neovim receives the key, it first moves between Neovim windows. If there is no Neovim window in that direction, it asks Herdr to focus the adjacent Herdr pane.

The helper forwards control keys with Herdr's raw `pane.send_text` API. This avoids bracketed paste, which would make Neovim try to insert text into buffers like `NvimTree`.

The helper can also own split bindings and keeps a short live layout cache under `~/.cache/herdr.nvim/`. This avoids waiting for Herdr's debounced `session.json` save before pane navigation knows about a new split.

The lazy.nvim example below builds the helper automatically. For a manual checkout, build it before using the plugin:

```sh
cargo build --release
```

The Neovim plugin uses `target/release/herdr-navigator` by default. Override `helper` in setup only if you install the binary somewhere else. If the helper is missing or not executable, the plugin warns and skips Herdr integration.

Neovim panes are not reported as persistent Herdr agents. The helper only uses Herdr's agent focus API internally as a temporary focus shim, then releases that marker immediately.

## Lazy.nvim

```lua
{
  "devxplay/herdr.nvim",
  build = "cargo build --release",
  config = function()
    require("herdr").setup()
  end,
}
```

## Herdr config

Install `herdr-navigator` somewhere on your `PATH` for Herdr shell keybindings:

```sh
cargo install --git https://github.com/devxplay/herdr.nvim.git --bin herdr-navigator
```

Keep prefix pane movement as a fallback:

```toml
focus_pane_left = "prefix+h"
focus_pane_down = "prefix+j"
focus_pane_up = "prefix+k"
focus_pane_right = "prefix+l"
```

Route split keys through the helper so new panes are immediately visible to navigation:

```toml
split_vertical = ""
split_horizontal = ""

[[keys.command]]
key = 'prefix+\'
type = "shell"
command = "herdr-navigator split right"

[[keys.command]]
key = "prefix+minus"
type = "shell"
command = "herdr-navigator split down"
```

Bind direct keys to the helper:

```toml
[[keys.command]]
key = "ctrl+h"
type = "shell"
command = "herdr-navigator dispatch left"

[[keys.command]]
key = "ctrl+j"
type = "shell"
command = "herdr-navigator dispatch down"

[[keys.command]]
key = "ctrl+k"
type = "shell"
command = "herdr-navigator dispatch up"

[[keys.command]]
key = "ctrl+l"
type = "shell"
command = "herdr-navigator dispatch right"
```

## Commands

- `:HerdrNavigateLeft`
- `:HerdrNavigateDown`
- `:HerdrNavigateUp`
- `:HerdrNavigateRight`
- `:HerdrRegisterPane`
- `:HerdrReleasePane`
