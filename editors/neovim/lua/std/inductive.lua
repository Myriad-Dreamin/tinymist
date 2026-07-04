---@generic S
---@alias Constructor fun(self: any, ...: any): S
---@alias InductiveMethod fun(self: S, ...: any): any

---@alias ConstructorDefs table<string, Constructor> | table<string, table<string, InductiveMethod>>

---@class Inductive<S> : { [string]: InductiveMethod }
---@operator call(table): `S`

---Create a new inductive type.
---@param name string The name of the new type, used only for errors
---@param defs ConstructorDefs A table of constructor definitions
---@return Inductive
return function(name, defs)
  local Type = {}

  local to_obj

  local _, first = next(defs)
  if type(first) ~= 'table' then
    to_obj = function(_, t)
      return t
    end
  else
    local methods = vim.tbl_keys(first)

    to_obj = function(constructor_name, impl)
      local obj = setmetatable({
        serialize = function(self)
          return { [constructor_name] = self[1] }
        end,
      }, { __index = Type })

      for _, method_name in ipairs(methods) do
        local method = impl[method_name]

        if not method then
          error(('%s method is missing for %s.%s'):format(method_name, name, constructor_name))
        end
        obj[method_name] = method
        impl[method_name] = nil -- so we can tell if there are any extras...
      end

      local extra = next(impl)
      if extra then
        error(('%s method is unexpected for %s.%s'):format(extra, name, constructor_name))
      end
      return function(_, ...)
        return setmetatable({ ... }, {
          __index = obj,
          __call = function(_, ...)
            return Type[constructor_name](Type, ...)
          end,
        })
      end
    end
  end

  for constructor_name, impl in pairs(defs) do
    Type[constructor_name] = to_obj(constructor_name, impl)
  end
  return setmetatable(Type, {
    __call = function(self, data, ...)
      local constructor_name, value = next(data)
      local constructor = self[constructor_name]
      if not constructor then
        error(('Invalid %s constructor: %s'):format(name, constructor_name))
      end
      return constructor(self, value, ...)
    end,
  })
end
