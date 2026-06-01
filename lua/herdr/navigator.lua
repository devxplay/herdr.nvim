local M = {}

local config = {
	helper = nil,
	set_keymaps = true,
	register_on_start = true,
}

local missing_helper_notified = false

local directions = {
	left = { key = "<C-h>", cmd = "HerdrNavigateLeft", wincmd = "h" },
	down = { key = "<C-j>", cmd = "HerdrNavigateDown", wincmd = "j" },
	up = { key = "<C-k>", cmd = "HerdrNavigateUp", wincmd = "k" },
	right = { key = "<C-l>", cmd = "HerdrNavigateRight", wincmd = "l" },
}

local function plugin_root()
	local source = debug.getinfo(1, "S").source:gsub("^@", "")
	return vim.fn.fnamemodify(source, ":h:h:h")
end

local function helper_path()
	if config.helper then
		return vim.fn.expand(config.helper)
	end
	return plugin_root() .. "/target/release/herdr-navigator"
end

local function in_herdr()
	return vim.env.HERDR_ENV == "1" or vim.env.HERDR_SOCKET_PATH ~= nil
end

local function run_helper(args)
	if not in_herdr() then
		return
	end

	local helper = helper_path()
	if vim.fn.executable(helper) ~= 1 then
		if not missing_helper_notified then
			vim.notify("herdr.nvim helper is not executable: " .. helper, vim.log.levels.WARN)
			missing_helper_notified = true
		end
		return
	end

	local command = vim.list_extend({ helper }, args)
	if vim.system then
		vim.system(command, { text = true }, function() end)
	else
		vim.fn.jobstart(command, { detach = true })
	end
end

function M.register()
	run_helper({ "register" })
end

function M.release()
	run_helper({ "release" })
end

function M.navigate(direction)
	local spec = directions[direction]
	if not spec then
		return
	end

	local current = vim.api.nvim_get_current_win()
	vim.cmd.wincmd(spec.wincmd)
	if vim.api.nvim_get_current_win() ~= current then
		return
	end

	run_helper({ "focus", direction })
end

function M.setup(opts)
	config = vim.tbl_deep_extend("force", config, opts or {})

	for direction, spec in pairs(directions) do
		vim.api.nvim_create_user_command(spec.cmd, function()
			M.navigate(direction)
		end, {})
	end

	vim.api.nvim_create_user_command("HerdrRegisterPane", M.register, {})
	vim.api.nvim_create_user_command("HerdrReleasePane", M.release, {})

	if config.set_keymaps then
		for direction, spec in pairs(directions) do
			vim.keymap.set("n", spec.key, function()
				M.navigate(direction)
			end, { silent = true, desc = "Navigate " .. direction .. " with Herdr" })
			vim.keymap.set("t", spec.key, "<C-\\><C-n><cmd>" .. spec.cmd .. "<cr>", {
				silent = true,
				desc = "Navigate " .. direction .. " with Herdr",
			})
		end
	end

	if config.register_on_start and in_herdr() then
		local group = vim.api.nvim_create_augroup("HerdrNvimPaneRegistration", { clear = true })
		vim.api.nvim_create_autocmd({ "VimEnter", "FocusGained", "WinEnter" }, {
			group = group,
			callback = M.register,
		})
		vim.api.nvim_create_autocmd("VimLeavePre", {
			group = group,
			callback = M.release,
		})
		for _, delay in ipairs({ 0, 100, 500, 1000 }) do
			vim.defer_fn(M.register, delay)
		end
	end
end

return M
