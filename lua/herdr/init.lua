local navigator = require("herdr.navigator")

local M = {}

local defaults = {
  helper = nil,
  set_keymaps = true,
  register_on_start = true,
}

function M.setup(opts)
  opts = vim.tbl_deep_extend("force", defaults, opts or {})
  navigator.setup(opts)
end

return M

