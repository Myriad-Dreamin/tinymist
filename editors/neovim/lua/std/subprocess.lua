local subprocess = {}

---Run a subprocess, blocking on exit, and returning its stdout.
---@return string: the lines of stdout of the exited process
function subprocess.check_output(...)
  local process = vim.system(...)
  local result = process:wait()
  if result.code == 0 then
    return result.stdout
  end

  error(
    string.format(
      '%s exited with non-zero exit status %d.\nstderr contained:\n%s',
      vim.inspect(process.cmd),
      result.code,
      result.stderr
    )
  )
end

return subprocess
