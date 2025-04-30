These again are dictionaries with the keys
- `description` (optional): The description for the argument.
- `types` (optional): A list of accepted argument types.
- `default` (optional): Default value for this argument.

See show-module() for outputting the results of this function.

- content (string): Content of `.typ` file to analyze for docstrings.
- name (string): The name for the module.
- label-prefix (auto, string): The label-prefix for internal function
  references. If `auto`, the label-prefix name will be the module name.
- require-all-parameters (boolean): Require that all parameters of a
  functions are documented and fail if some are not.
- scope (dictionary): A dictionary of definitions that are then available
  in all function and parameter descriptions.
- preamble (string): Code to prepend to all code snippets shown with `#example()`.
  This can for instance be used to import something from the scope.
-> string
