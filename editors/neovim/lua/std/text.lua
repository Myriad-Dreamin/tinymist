local text = {}

local function max_common_indent(str)
  local _, _, common_indent, rest = str:find '^(%s*)(.*)'
  local common_indent_len = #common_indent
  local len
  for indent in rest:gmatch '\n( +)' do
    len = #indent
    if len < common_indent_len then
      common_indent, common_indent_len = indent, len
    end
  end
  return common_indent
end

---Dedent a multi-line string.
---@param str string
function text.dedent(str)
  str = str:gsub('\n *$', '\n') -- trim leading/trailing space
  local prefix = max_common_indent(str)
  return str:gsub('^' .. prefix, ''):gsub('\n' .. prefix, '\n')
end

---Build a single-line string out a multiline one, replacing \n with spaces.
function text.s(str)
  return text.dedent(str):gsub('\n', ' ')
end

return text
