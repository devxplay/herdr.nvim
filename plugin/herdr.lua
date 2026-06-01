if vim.g.loaded_herdr_nvim == 1 then
	return
end

vim.g.loaded_herdr_nvim = 1

vim.schedule(function()
	local herdr = require("herdr")
	if not herdr._did_setup then
		herdr.setup()
	end
end)
