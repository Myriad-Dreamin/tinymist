---@brief [[
--- Editor-level DAP smoke specs.
---@brief ]]

local assert = require 'luassert'
local dap = require 'dap'

local function request(session, command, args, callback)
  session:request(command, args, function(err, response)
    if err then
      callback(('DAP %s failed: %s'):format(command, vim.inspect(err)))
      return
    end

    callback(nil, response)
  end)
end

local function continue_debug(session, thread_id, callback)
  request(session, 'continue', { threadId = thread_id or 1 }, callback or function() end)
end

local function prepare_program(name, lines)
  local root = vim.fs.joinpath(vim.uv.cwd(), 'target', 'dap-spec', name)
  local program = vim.fs.joinpath(root, 'main.typ')

  vim.fn.mkdir(root, 'p')
  vim.fn.writefile(lines, program)

  vim.cmd.edit(vim.fn.fnameescape(program))
  vim.bo.filetype = 'typst'

  dap.adapters.tinymist = {
    type = 'executable',
    command = 'tinymist',
    args = { 'dap', '--mirror', vim.fs.joinpath(root, 'mirror.log') },
  }

  return root, program
end

local function cleanup_listener(key)
  dap.listeners.after.event_stopped[key] = nil
end

local function finish_session()
  local session = dap.session()
  if not session then
    return
  end

  session:request('continue', { threadId = 1 }, function() end)
  session:request('disconnect', { terminateDebuggee = true }, function() end)
  vim.wait(1000, function()
    return dap.session() == nil
  end, 50)
end

describe('DAP breakpoints', function()
  it('stops at document end and evaluates module scope', function()
    local root, program = prepare_program('document-end', {
      '#let answer = 40 + 2',
      '#answer',
    })
    local key = 'tinymist-dap-document-end'
    local stopped = {}
    local result
    local failure

    cleanup_listener(key)
    dap.listeners.after.event_stopped[key] = function(session, body)
      stopped[#stopped + 1] = body.reason or '<unknown>'

      if body.reason == 'entry' then
        continue_debug(session, body.threadId, function(err)
          failure = failure or err
        end)
        return
      end

      if body.reason == 'pause' then
        request(session, 'evaluate', {
          expression = 'answer',
          context = 'repl',
          frameId = 1,
        }, function(err, response)
          failure = failure or err
          result = response and response.result or result
        end)
      end
    end

    dap.run {
      type = 'tinymist',
      request = 'launch',
      name = 'Tinymist document-end DAP spec',
      program = program,
      root = root,
      stopOnEntry = true,
    }

    local ok = vim.wait(20000, function()
      return failure ~= nil or result ~= nil
    end, 50)

    cleanup_listener(key)
    finish_session()

    assert.message(('timed out waiting for document-end pause; stopped=%s'):format(vim.inspect(stopped))).is_true(ok)
    assert.message(failure or 'document-end DAP flow failed').is_nil(failure)
    assert.are.same('42', result)
    assert.are.same({ 'entry', 'pause' }, stopped)
  end)

  it('stops at a named function breakpoint and evaluates function scope', function()
    local root, program = prepare_program('function-breakpoint', {
      '#let add(x, y: 2) = x + y',
      '#let answer = add(40, y: 2)',
      '#answer',
    })
    local key = 'tinymist-dap-function-breakpoint'
    local stopped = {}
    local configured = false
    local function_value
    local function_stack
    local end_value
    local failure

    cleanup_listener(key)
    dap.listeners.after.event_stopped[key] = function(session, body)
      stopped[#stopped + 1] = body.reason or '<unknown>'

      if body.reason == 'entry' then
        request(session, 'setFunctionBreakpoints', {
          breakpoints = {
            { name = 'add' },
          },
        }, function(err, response)
          failure = failure or err
          configured = response ~= nil
          continue_debug(session, body.threadId, function(continue_err)
            failure = failure or continue_err
          end)
        end)
        return
      end

      if body.reason == 'function breakpoint' then
        local remaining = 2
        local function maybe_continue(err)
          failure = failure or err
          remaining = remaining - 1
          if remaining == 0 then
            continue_debug(session, body.threadId, function(continue_err)
              failure = failure or continue_err
            end)
          end
        end

        request(session, 'evaluate', {
          expression = 'x + y',
          context = 'repl',
          frameId = 1,
        }, function(err, response)
          function_value = response and response.result or function_value
          maybe_continue(err)
        end)

        request(session, 'stackTrace', {
          threadId = body.threadId or 1,
        }, function(err, response)
          function_stack = response and response.stackFrames and response.stackFrames[1] or function_stack
          maybe_continue(err)
        end)
        return
      end

      if body.reason == 'pause' then
        request(session, 'evaluate', {
          expression = 'answer',
          context = 'repl',
          frameId = 1,
        }, function(err, response)
          failure = failure or err
          end_value = response and response.result or end_value
        end)
      end
    end

    dap.run {
      type = 'tinymist',
      request = 'launch',
      name = 'Tinymist function breakpoint DAP spec',
      program = program,
      root = root,
      stopOnEntry = true,
    }

    local ok = vim.wait(20000, function()
      return failure ~= nil or end_value ~= nil
    end, 50)

    cleanup_listener(key)
    finish_session()

    assert.message(('timed out waiting for function breakpoint flow; stopped=%s'):format(vim.inspect(stopped))).is_true(ok)
    assert.message(failure or 'function breakpoint DAP flow failed').is_nil(failure)
    assert.is_true(configured)
    assert.are.same('42', function_value)
    assert.are.same('42', end_value)
    assert.are.same({ 'entry', 'function breakpoint', 'pause' }, stopped)
    assert.is_not_nil(function_stack)
    assert.are.same(1, function_stack.line)
    assert.are.same(program, function_stack.source.path)
  end)
end)
