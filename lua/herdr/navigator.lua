local M = {}

local config = {
	helper = nil,
	set_keymaps = true,
	register_on_start = true,
}

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

local function in_tmux()
	return vim.env.TMUX ~= nil and vim.env.TMUX ~= ""
end

local function has_tmux_navigator()
	return vim.fn.exists(":TmuxNavigateLeft") == 2
end

local tmux_commands = {
	left = "TmuxNavigateLeft",
	down = "TmuxNavigateDown",
	up = "TmuxNavigateUp",
	right = "TmuxNavigateRight",
}

local compiling = false
local pending_calls = {}

local function compile_helper(callback)
	if compiling then
		if callback then
			table.insert(pending_calls, callback)
		end
		return
	end
	compiling = true
	if callback then
		table.insert(pending_calls, callback)
	end

	vim.notify("herdr.nvim: building herdr-navigator...", vim.log.levels.INFO)

	local cmd = { "cargo", "build", "--release" }
	local root = plugin_root()

	vim.system(cmd, { cwd = root }, function(obj)
		vim.schedule(function()
			compiling = false
			if obj.code == 0 then
				vim.notify("herdr.nvim: herdr-navigator built successfully", vim.log.levels.INFO)
				local calls = pending_calls
				pending_calls = {}
				for _, cb in ipairs(calls) do
					pcall(cb)
				end
			else
				vim.notify("herdr.nvim: failed to build herdr-navigator\n" .. (obj.stderr or ""), vim.log.levels.ERROR)
				pending_calls = {}
			end
		end)
	end)
end

local function run_helper(args)
	if not in_herdr() then
		return
	end

	local helper = helper_path()
	if vim.fn.executable(helper) ~= 1 then
		compile_helper(function()
			run_helper(args)
		end)
		return
	end

	local command = vim.list_extend({ helper }, args)
	if vim.system then
		vim.system(command, { text = true }, function() end)
	else
		vim.fn.jobstart(command, { detach = true })
	end
end

local function pane_id()
	return vim.env.HERDR_ACTIVE_PANE_ID or vim.env.HERDR_PANE_ID
end

local function nvim_panes_dir()
	local base = vim.env.XDG_CACHE_HOME or (vim.env.HOME .. "/.cache")
	return base .. "/herdr.nvim/panes"
end

function M.register()
	if not in_herdr() then
		return
	end
	local id = pane_id()
	if not id or id == "" then
		return
	end
	local dir = nvim_panes_dir()
	vim.fn.mkdir(dir, "p")
	local f = io.open(dir .. "/" .. id, "w")
	if f then
		f:close()
	end
end

function M.release()
	if not in_herdr() then
		return
	end
	local id = pane_id()
	if not id or id == "" then
		return
	end
	os.remove(nvim_panes_dir() .. "/" .. id)
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

	if in_herdr() then
		run_helper({ "focus", direction })
	elseif in_tmux() and has_tmux_navigator() then
		vim.cmd(tmux_commands[direction])
	end
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
		-- Suppress vim-tmux-navigator's own keymaps since herdr.nvim takes
		-- over Ctrl+h/j/k/l and delegates to TmuxNavigate* when appropriate.
		vim.g.tmux_navigator_no_mappings = 1

		for direction, spec in pairs(directions) do
			vim.keymap.set("n", spec.key, function()
				M.navigate(direction)
			end, { silent = true, desc = "Navigate " .. direction })
			vim.keymap.set("t", spec.key, "<C-\\><C-n><cmd>" .. spec.cmd .. "<cr>", {
				silent = true,
				desc = "Navigate " .. direction,
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
