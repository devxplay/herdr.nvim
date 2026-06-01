local obj = vim.system({ "cargo", "build", "--release" }, {
	cwd = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":h"),
}):wait()

if obj.code ~= 0 then
	error("herdr-navigator: cargo build failed\n" .. (obj.stderr or ""))
end
