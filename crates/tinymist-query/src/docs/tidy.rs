use ecow::EcoString;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use typst::diag::StrResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidyParamDocs {
    pub name: EcoString,
    pub docs: EcoString,
    pub types: EcoString,
    pub default: Option<EcoString>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidyPatDocs {
    pub docs: EcoString,
    pub return_ty: Option<EcoString>,
    pub params: Vec<TidyParamDocs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidyModuleDocs {
    pub docs: EcoString,
}

pub fn identify_pat_docs(converted: &str) -> StrResult<TidyPatDocs> {
    let lines = converted.lines().collect::<Vec<_>>();

    let mut matching_return_ty = true;
    let mut buf = vec![];
    let mut params = vec![];
    let mut return_ty = None;
    let mut break_line = None;

    let mut i = lines.len();
    'search: loop {
        if i == 0 {
            break;
        }
        i -= 1;

        let line = lines[i];
        if line.is_empty() {
            continue;
        }

        loop {
            if matching_return_ty {
                matching_return_ty = false;
                let Some(w) = line.trim_start().strip_prefix("->") else {
                    // break_line = Some(i);
                    continue;
                };

                break_line = Some(i);
                return_ty = Some(w.trim().into());
                break;
            }

            let Some(mut line) = line
                .trim_end()
                .strip_suffix("<!-- typlite:end:list-item 0 -->")
            else {
                break_line = Some(i + 1);
                break 'search;
            };
            let mut current_line_no = i;

            loop {
                // <!-- typlite:begin:list-item -->
                let t = line
                    .trim_start()
                    .strip_prefix("- ")
                    .and_then(|t| t.trim().strip_prefix("<!-- typlite:begin:list-item 0 -->"));

                let line_content = match t {
                    Some(t) => {
                        buf.push(t);
                        break;
                    }
                    None => line,
                };

                buf.push(line_content);

                if current_line_no == 0 {
                    break_line = Some(i + 1);
                    break 'search;
                }
                current_line_no -= 1;
                line = lines[current_line_no];
            }

            let mut buf = std::mem::take(&mut buf);
            buf.reverse();

            let Some(first_line) = buf.first_mut() else {
                break_line = Some(i + 1);
                break 'search;
            };
            *first_line = first_line.trim();

            let Some(param_line) = None.or_else(|| {
                let (param_name, rest) = first_line.split_once(" ")?;
                let (type_content, rest) = match_brace(rest.trim_start().strip_prefix("(")?)?;
                let (_, rest) = rest.split_once(":")?;
                *first_line = rest.trim();
                Some((param_name.into(), type_content.into()))
            }) else {
                break_line = Some(i + 1);
                break 'search;
            };

            i = current_line_no;
            params.push(TidyParamDocs {
                name: param_line.0,
                types: param_line.1,
                default: None,
                docs: buf.into_iter().join("\n").into(),
            });

            break;
        }
    }

    let docs = match break_line {
        Some(line_no) => (lines[..line_no]).iter().copied().join("\n").into(),
        None => converted.into(),
    };

    params.reverse();
    Ok(TidyPatDocs {
        docs,
        return_ty,
        params,
    })
}

pub fn identify_tidy_module_docs(docs: EcoString) -> StrResult<TidyModuleDocs> {
    Ok(TidyModuleDocs { docs })
}

fn match_brace(trim_start: &str) -> Option<(&str, &str)> {
    let mut brace_count = 1;
    let mut end = 0;
    for (i, c) in trim_start.char_indices() {
        match c {
            '(' => brace_count += 1,
            ')' => brace_count -= 1,
            _ => {}
        }

        if brace_count == 0 {
            end = i;
            break;
        }
    }

    if brace_count != 0 {
        return None;
    }

    let (type_content, rest) = trim_start.split_at(end);
    Some((type_content, rest))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::TidyParamDocs;

    fn func(s: &str) -> String {
        let f = super::identify_pat_docs(s).unwrap();
        let mut res = format!(">> docs:\n{}\n<< docs", f.docs);
        if let Some(t) = f.return_ty {
            res.push_str(&format!("\n>>return\n{t}\n<<return"));
        }
        for TidyParamDocs {
            name,
            types,
            docs,
            default: _,
        } in f.params
        {
            let _ = write!(res, "\n>>arg {name}: {types}\n{docs}\n<< arg");
        }
        res
    }

    fn var(s: &str) -> String {
        let f = super::identify_pat_docs(s).unwrap();
        let mut res = format!(">> docs:\n{}\n<< docs", f.docs);
        if let Some(t) = f.return_ty {
            res.push_str(&format!("\n>>return\n{t}\n<<return"));
        }
        res
    }

    #[test]
    fn test_identify_tidy_docs() {
        insta::assert_snapshot!(func(r###"These again are dictionaries with the keys
- <!-- typlite:begin:list-item 0 -->`description` (optional): The description for the argument.<!-- typlite:end:list-item 0 -->
- <!-- typlite:begin:list-item 0 -->`types` (optional): A list of accepted argument types.<!-- typlite:end:list-item 0 --> 
- <!-- typlite:begin:list-item 0 -->`default` (optional): Default value for this argument.<!-- typlite:end:list-item 0 -->

See @@show-module() for outputting the results of this function.

- <!-- typlite:begin:list-item 0 -->content (string): Content of `.typ` file to analyze for docstrings.<!-- typlite:end:list-item 0 -->
- <!-- typlite:begin:list-item 0 -->name (string): The name for the module.<!-- typlite:end:list-item 0 --> 
- <!-- typlite:begin:list-item 0 -->label-prefix (auto, string): The label-prefix for internal function 
        references. If `auto`, the label-prefix name will be the module name.<!-- typlite:end:list-item 0 --> 
- <!-- typlite:begin:list-item 0 -->require-all-parameters (boolean): Require that all parameters of a 
        functions are documented and fail if some are not.<!-- typlite:end:list-item 0 --> 
- <!-- typlite:begin:list-item 0 -->scope (dictionary): A dictionary of definitions that are then available 
        in all function and parameter descriptions.<!-- typlite:end:list-item 0 --> 
- <!-- typlite:begin:list-item 0 -->preamble (string): Code to prepend to all code snippets shown with `#example()`. 
        This can for instance be used to import something from the scope.<!-- typlite:end:list-item 0 --> 
-> string"###), @r###"
        >> docs:
        These again are dictionaries with the keys
        - <!-- typlite:begin:list-item 0 -->`description` (optional): The description for the argument.<!-- typlite:end:list-item 0 -->
        - <!-- typlite:begin:list-item 0 -->`types` (optional): A list of accepted argument types.<!-- typlite:end:list-item 0 --> 
        - <!-- typlite:begin:list-item 0 -->`default` (optional): Default value for this argument.<!-- typlite:end:list-item 0 -->

        See @@show-module() for outputting the results of this function.
        << docs
        >>return
        string
        <<return
        >>arg content: string
        Content of `.typ` file to analyze for docstrings.
        << arg
        >>arg name: string
        The name for the module.
        << arg
        >>arg label-prefix: auto, string
        The label-prefix for internal function
                references. If `auto`, the label-prefix name will be the module name.
        << arg
        >>arg require-all-parameters: boolean
        Require that all parameters of a
                functions are documented and fail if some are not.
        << arg
        >>arg scope: dictionary
        A dictionary of definitions that are then available
                in all function and parameter descriptions.
        << arg
        >>arg preamble: string
        Code to prepend to all code snippets shown with `#example()`.
                This can for instance be used to import something from the scope.
        << arg
        "###);
    }

    #[test]
    fn test_identify_tidy_docs_nested() {
        insta::assert_snapshot!(func(r###"These again are dictionaries with the keys
- <!-- typlite:begin:list-item 0 -->`description` (optional): The description for the argument.<!-- typlite:end:list-item 0 -->

See @@show-module() for outputting the results of this function.

- <!-- typlite:begin:list-item 0 -->name (string): The name for the module.<!-- typlite:end:list-item 0 --> 
- <!-- typlite:begin:list-item 0 -->label-prefix (auto, string): The label-prefix for internal function 
        references. If `auto`, the label-prefix name will be the module name. 
  - <!-- typlite:begin:list-item 1 -->nested something<!-- typlite:end:list-item 1 -->
  - <!-- typlite:begin:list-item 1 -->nested something 2<!-- typlite:end:list-item 1 --><!-- typlite:end:list-item 0 -->
-> string"###), @r###"
        >> docs:
        These again are dictionaries with the keys
        - <!-- typlite:begin:list-item 0 -->`description` (optional): The description for the argument.<!-- typlite:end:list-item 0 -->

        See @@show-module() for outputting the results of this function.
        << docs
        >>return
        string
        <<return
        >>arg name: string
        The name for the module.
        << arg
        >>arg label-prefix: auto, string
        The label-prefix for internal function
                references. If `auto`, the label-prefix name will be the module name. 
          - <!-- typlite:begin:list-item 1 -->nested something<!-- typlite:end:list-item 1 -->
          - <!-- typlite:begin:list-item 1 -->nested something 2<!-- typlite:end:list-item 1 -->
        << arg
        "###);
    }

    #[test]
    fn test_identify_tidy_docs3() {
        insta::assert_snapshot!(var(r###"See @@show-module() for outputting the results of this function.
-> string"###), @r###"
        >> docs:
        See @@show-module() for outputting the results of this function.
        << docs
        >>return
        string
        <<return
        "###);
    }

    #[test]
    fn test_identify_tidy_docs4() {
        insta::assert_snapshot!(var(r###"
- <!-- typlite:begin:list-item 0 -->name (string): The name for the module.<!-- typlite:end:list-item 0 --> 
-> string"###), @r###"
        >> docs:

        - <!-- typlite:begin:list-item 0 -->name (string): The name for the module.<!-- typlite:end:list-item 0 --> 
        << docs
        >>return
        string
        <<return
        "###);
    }
}
