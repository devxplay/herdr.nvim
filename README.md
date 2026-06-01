# herdr.nvim

Neovim integration for [Herdr](https://herdr.dev).

The first feature is tmux-style navigation between Neovim windows and Herdr panes with one key family:

- `Ctrl+h`
- `Ctrl+j`
- `Ctrl+k`
- `Ctrl+l`

## How it works

Herdr does not currently expose tmux-style process-aware key forwarding. `herdr.nvim` works around that with two pieces:

1. The Neovim plugin writes a per-pane marker file under `~/.cache/herdr.nvim/panes/` while Neovim is open. This is a plain empty file named after the Herdr pane ID — no JSON, no shared state, no races.
2. The Rust `herdr-navigator` helper is bound in Herdr. When `Ctrl+h/j/k/l` is pressed:
   - if the active pane has a marker file, it forwards the key into Neovim;
   - otherwise it moves focus to the adjacent Herdr pane.

When Neovim receives the key, it first moves between Neovim windows. If there is no Neovim window in that direction, it asks Herdr to focus the adjacent Herdr pane.

The helper forwards control keys with Herdr's raw `pane.send_text` API. This avoids bracketed paste, which would make Neovim try to insert text into buffers like `NvimTree`.

The helper can also own split bindings and keeps a short live layout cache under `~/.cache/herdr.nvim/`. This avoids waiting for Herdr's debounced `session.json` save before pane navigation knows about a new split.

Stale marker files (from crashed Neovim instances) are pruned automatically by checking against Herdr's live `pane.list` at dispatch time.

The plugin builds the Rust helper automatically on install/update (via `build.lua` for lazy.nvim, or on-demand at runtime for other package managers). For a manual checkout, build it yourself:

```sh
cargo build --release
```

The Neovim plugin uses `target/release/herdr-navigator` by default. Override `helper` in setup only if you install the binary somewhere else.

Neovim pane registration uses simple marker files, not Herdr's agent API. The helper only uses Herdr's agent focus API internally as a temporary focus shim (since Herdr has no direct `pane.focus` API), then releases that marker immediately.

## Coexistence with vim-tmux-navigator

`herdr.nvim` detects the environment at runtime:

- **Inside Herdr** (`$HERDR_ENV` or `$HERDR_SOCKET_PATH` set): always uses its own navigator.
- **Inside tmux** (`$TMUX` set) with [vim-tmux-navigator](https://github.com/christoomey/vim-tmux-navigator) installed: delegates `Ctrl+h/j/k/l` to `TmuxNavigate*` commands when at a Neovim edge.
- **Neither**: navigation stays within Neovim windows only.

This means you can have both plugins in your config with no special flags:

```lua
-- plugins/tmux.lua
return {
  "christoomey/vim-tmux-navigator",
}

-- plugins/herdr.lua
return {
  "devxplay/herdr.nvim",
}
```

## Lazy.nvim

```lua
{
  "devxplay/herdr.nvim",
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
