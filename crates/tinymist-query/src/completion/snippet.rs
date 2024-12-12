use std::sync::OnceLock;

use ecow::{eco_format, EcoString};
use hashbrown::HashSet;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use strum::IntoEnumIterator;

use crate::prelude::*;
use crate::syntax::InterpretMode;
use crate::ty::Interned;

/// This is the poorman's type filter, which is less powerful but more steady.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PostfixSnippetScope {
    /// Any "dottable" value, i.e. having type `Ty::Any`.
    Value,
    /// Any value having content type, i.e. having type `Ty::Content`.
    Content,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CompletionCommand {
    TriggerSuggest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, strum::EnumIter)]
pub enum SurroundingSyntax {
    Regular,
    StringContent,
    Selector,
    ShowTransform,
    ImportList,
    SetRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContextSelector<T> {
    Positive(Option<T>),
    Negative(Vec<T>),
}

impl<T> Default for ContextSelector<T> {
    fn default() -> Self {
        ContextSelector::Positive(None)
    }
}

impl<'de, T> Deserialize<'de> for ContextSelector<T>
where
    T: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Null => Ok(ContextSelector::Positive(None)),
            serde_json::Value::Object(map) => {
                let negative = map
                    .get("negative")
                    .ok_or_else(|| serde::de::Error::custom("missing field `negative`"))?;
                let negative = serde_json::from_value(negative.clone())
                    .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                Ok(ContextSelector::Negative(negative))
            }
            _ => {
                let value = serde_json::from_value(value)
                    .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                Ok(ContextSelector::Positive(Some(value)))
            }
        }
    }
}

impl<T> Serialize for ContextSelector<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            ContextSelector::Positive(value) => {
                if let Some(value) = value {
                    value.serialize(serializer)
                } else {
                    serde_json::Value::Null.serialize(serializer)
                }
            }
            ContextSelector::Negative(value) => {
                let mut map = serde_json::Map::new();
                map.insert("negative".into(), serde_json::to_value(value).unwrap());
                serde_json::Value::Object(map).serialize(serializer)
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CompletionContext {
    /// The mode in which the snippet is applicable.
    pub mode: ContextSelector<InterpretMode>,
    /// The syntax in which the snippet is applicable.
    pub syntax: ContextSelector<SurroundingSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionContextKeyRepr {
    /// The mode in which the snippet is applicable.
    pub mode: Option<InterpretMode>,
    /// The syntax in which the snippet is applicable.
    pub syntax: Option<SurroundingSyntax>,
}

crate::adt::interner::impl_internable!(CompletionContextKeyRepr,);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionContextKey(Interned<CompletionContextKeyRepr>);

impl CompletionContextKey {
    /// Creates a new completion context key.
    pub fn new(mode: Option<InterpretMode>, syntax: Option<SurroundingSyntax>) -> Self {
        CompletionContextKey(Interned::new(CompletionContextKeyRepr { mode, syntax }))
    }
}

/// A parsed snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSnippet {
    pub node_before: EcoString,
    pub node_before_before_cursor: Option<EcoString>,
    pub node_after: EcoString,
}

/// A postfix completion snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostfixSnippet {
    /// The mode in which the snippet is applicable.
    pub mode: EcoVec<InterpretMode>,
    /// The scope in which the snippet is applicable.
    pub scope: PostfixSnippetScope,
    /// The snippet name.
    pub label: EcoString,
    /// The detailed snippet name shown in UI (might be truncated).
    pub label_detail: Option<EcoString>,
    /// The snippet content.
    pub snippet: EcoString,
    /// The snippet description.
    pub description: EcoString,
    /// Lazily parsed snippet.
    #[serde(skip)]
    pub parsed_snippet: OnceLock<Option<ParsedSnippet>>,
}

/// A prefix completion snippet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrefixSnippet {
    /// The mode in which the snippet is applicable.
    pub context: EcoVec<CompletionContext>,
    /// The snippet name.
    pub label: EcoString,
    /// The detailed snippet name shown in UI (might be truncated).
    pub label_detail: Option<EcoString>,
    /// The snippet content.
    pub snippet: EcoString,
    /// The snippet description.
    pub description: EcoString,
    /// The command to execute.
    pub command: Option<CompletionCommand>,
    /// Lazily expanded context.
    #[serde(skip)]
    pub expanded_context: OnceLock<HashSet<CompletionContextKey>>,
}
crate::adt::interner::impl_internable!(PrefixSnippet,);

impl std::hash::Hash for PrefixSnippet {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.context.hash(state);
        self.label.hash(state);
    }
}

impl std::cmp::Eq for PrefixSnippet {}
impl std::cmp::PartialEq for PrefixSnippet {
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context && self.label == other.label
    }
}

impl PrefixSnippet {
    pub(crate) fn applies_to(&self, context_key: &CompletionContextKey) -> bool {
        self.expanded_context
            .get_or_init(|| {
                let mut set = HashSet::new();
                for context in &self.context {
                    let modes = match &context.mode {
                        ContextSelector::Positive(mode) => vec![*mode],
                        ContextSelector::Negative(modes) => {
                            let all_modes = InterpretMode::iter()
                                .filter(|mode| !modes.iter().any(|m| m == mode));
                            all_modes.map(Some).collect()
                        }
                    };
                    let syntaxes = match &context.syntax {
                        ContextSelector::Positive(syntax) => vec![*syntax],
                        ContextSelector::Negative(syntaxes) => {
                            let all_syntaxes = SurroundingSyntax::iter()
                                .filter(|syntax| !syntaxes.iter().any(|s| s == syntax));
                            all_syntaxes.map(Some).collect()
                        }
                    };
                    for mode in &modes {
                        for syntax in &syntaxes {
                            set.insert(CompletionContextKey::new(*mode, *syntax));
                        }
                    }
                }
                set
            })
            .contains(context_key)
    }
}

struct ConstPrefixSnippet {
    context: InterpretMode,
    label: &'static str,
    snippet: &'static str,
    description: &'static str,
}

impl From<&ConstPrefixSnippet> for Interned<PrefixSnippet> {
    fn from(snippet: &ConstPrefixSnippet) -> Self {
        Interned::new(PrefixSnippet {
            context: eco_vec![CompletionContext {
                mode: ContextSelector::Positive(Some(snippet.context)),
                syntax: ContextSelector::Positive(None),
            }],
            label: snippet.label.into(),
            label_detail: None,
            snippet: snippet.snippet.into(),
            description: snippet.description.into(),
            command: None,
            expanded_context: OnceLock::new(),
        })
    }
}

struct ConstPrefixSnippetWithSuggest {
    context: InterpretMode,
    label: &'static str,
    snippet: &'static str,
    description: &'static str,
}

impl From<&ConstPrefixSnippetWithSuggest> for Interned<PrefixSnippet> {
    fn from(snippet: &ConstPrefixSnippetWithSuggest) -> Self {
        Interned::new(PrefixSnippet {
            context: eco_vec![CompletionContext {
                mode: ContextSelector::Positive(Some(snippet.context)),
                syntax: ContextSelector::Positive(None),
            }],
            label: snippet.label.into(),
            label_detail: None,
            snippet: snippet.snippet.into(),
            description: snippet.description.into(),
            command: Some(CompletionCommand::TriggerSuggest),
            expanded_context: OnceLock::new(),
        })
    }
}

pub static DEFAULT_PREFIX_SNIPPET: LazyLock<Vec<Interned<PrefixSnippet>>> = LazyLock::new(|| {
    const SNIPPETS: &[ConstPrefixSnippet] = &[
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "function call",
            snippet: "${function}(${arguments})[${body}]",
            description: "Evaluates a function.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "code block",
            snippet: "{ ${} }",
            description: "Inserts a nested code block.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "content block",
            snippet: "[${content}]",
            description: "Switches into markup mode.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "set rule",
            snippet: "set ${}",
            description: "Sets style properties on an element.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "show rule",
            snippet: "show ${}",
            description: "Redefines the look of an element.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "show rule (everything)",
            snippet: "show: ${}",
            description: "Transforms everything that follows.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "context expression",
            snippet: "context ${}",
            description: "Provides contextual data.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "let binding",
            snippet: "let ${name} = ${value}",
            description: "Saves a value in a variable.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "let binding (function)",
            snippet: "let ${name}(${params}) = ${output}",
            description: "Defines a function.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "if conditional",
            snippet: "if ${1 < 2} {\n\t${}\n}",
            description: "Computes or inserts something conditionally.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "if-else conditional",
            snippet: "if ${1 < 2} {\n\t${}\n} else {\n\t${}\n}",
            description: "Computes or inserts different things based on a condition.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "while loop",
            snippet: "while ${1 < 2} {\n\t${}\n}",
            description: "Computes or inserts something while a condition is met.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "for loop",
            snippet: "for ${value} in ${(1, 2, 3)} {\n\t${}\n}",
            description: "Computes or inserts something for each value in a collection.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "for loop (with key)",
            snippet: "for (${key}, ${value}) in ${(a: 1, b: 2)} {\n\t${}\n}",
            description: "Computes or inserts something for each key and value in a collection.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "break",
            snippet: "break",
            description: "Exits early from a loop.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "continue",
            snippet: "continue",
            description: "Continues with the next iteration of a loop.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "return",
            snippet: "return ${output}",
            description: "Returns early from a function.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "array literal",
            snippet: "(${1, 2, 3})",
            description: "Creates a sequence of values.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Code,
            label: "dictionary literal",
            snippet: "(${a: 1, b: 2})",
            description: "Creates a mapping from names to value.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Math,
            label: "subscript",
            snippet: "${x}_${2:2}",
            description: "Sets something in subscript.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Math,
            label: "superscript",
            snippet: "${x}^${2:2}",
            description: "Sets something in superscript.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Math,
            label: "fraction",
            snippet: "${x}/${y}",
            description: "Inserts a fraction.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "expression",
            snippet: "#${}",
            description: "Variables, function calls, blocks, and more.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "linebreak",
            snippet: "\\\n${}",
            description: "Inserts a forced linebreak.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "strong text",
            snippet: "*${strong}*",
            description: "Strongly emphasizes content by increasing the font weight.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "emphasized text",
            snippet: "_${emphasized}_",
            description: "Emphasizes content by setting it in italic font style.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "raw text",
            snippet: "`${text}`",
            description: "Displays text verbatim, in monospace.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "code listing",
            snippet: "```${lang}\n${code}\n```",
            description: "Inserts computer code with syntax highlighting.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "hyperlink",
            snippet: "https://${example.com}",
            description: "Links to a URL.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "label",
            snippet: "<${name}>",
            description: "Makes the preceding element referenceable.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "reference",
            snippet: "@${name}",
            description: "Inserts a reference to a label.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "heading",
            snippet: "= ${title}",
            description: "Inserts a section heading.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "list item",
            snippet: "- ${item}",
            description: "Inserts an item of a bullet list.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "enumeration item",
            snippet: "+ ${item}",
            description: "Inserts an item of a numbered list.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "enumeration item (numbered)",
            snippet: "${number}. ${item}",
            description: "Inserts an explicitly numbered list item.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "term list item",
            snippet: "/ ${term}: ${description}",
            description: "Inserts an item of a term list.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "math (inline)",
            snippet: "$${x}$",
            description: "Inserts an inline-level mathematical equation.",
        },
        ConstPrefixSnippet {
            context: InterpretMode::Markup,
            label: "math (block)",
            snippet: "$ ${sum_x^2} $",
            description: "Inserts a block-level mathematical equation.",
        },
    ];

    const SNIPPET_SUGGEST: &[ConstPrefixSnippetWithSuggest] = &[
        ConstPrefixSnippetWithSuggest {
            context: InterpretMode::Code,
            label: "import module",
            snippet: "import \"${}\"",
            description: "Imports module from another file.",
        },
        ConstPrefixSnippetWithSuggest {
            context: InterpretMode::Code,
            label: "import module by expression",
            snippet: "import ${}",
            description: "Imports items by expression.",
        },
        ConstPrefixSnippetWithSuggest {
            context: InterpretMode::Code,
            label: "import (package)",
            snippet: "import \"@${}\"",
            description: "Imports variables from another file.",
        },
        ConstPrefixSnippetWithSuggest {
            context: InterpretMode::Code,
            label: "include (file)",
            snippet: "include \"${}\"",
            description: "Includes content from another file.",
        },
        ConstPrefixSnippetWithSuggest {
            context: InterpretMode::Code,
            label: "include (package)",
            snippet: "include \"@${}\"",
            description: "Includes content from another file.",
        },
    ];

    let snippets = SNIPPETS.iter().map(From::from);
    let snippets2 = SNIPPET_SUGGEST.iter().map(From::from);
    snippets.chain(snippets2).collect()
});

pub static DEFAULT_POSTFIX_SNIPPET: LazyLock<Vec<PostfixSnippet>> = LazyLock::new(|| {
    vec![
        PostfixSnippet {
            scope: PostfixSnippetScope::Content,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: eco_format!("text fill"),
            label_detail: Some(eco_format!(".text fill")),
            snippet: "text(fill: ${}, ${node})".into(),
            description: eco_format!("wrap with text fill"),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Content,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: eco_format!("text size"),
            label_detail: Some(eco_format!(".text size")),
            snippet: "text(size: ${}, ${node})".into(),
            description: eco_format!("wrap with text size"),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Content,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: eco_format!("align"),
            label_detail: Some(eco_format!(".align")),
            snippet: "align(${}, ${node})".into(),
            description: eco_format!("wrap with alignment"),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "if".into(),
            label_detail: Some(".if".into()),
            snippet: "if ${node} { ${} }".into(),
            description: "wrap as if expression".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "else".into(),
            label_detail: Some(".else".into()),
            snippet: "if not ${node} { ${} }".into(),
            description: "wrap as if not expression".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "none".into(),
            label_detail: Some(".if none".into()),
            snippet: "if ${node} == none { ${} }".into(),
            description: "wrap as if expression to check none-ish".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "notnone".into(),
            label_detail: Some(".if not none".into()),
            snippet: "if ${node} != none { ${} }".into(),
            description: "wrap as if expression to check none-ish".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "return".into(),
            label_detail: Some(".return".into()),
            snippet: "return ${node}".into(),
            description: "wrap as return expression".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "tup".into(),
            label_detail: Some(".tup".into()),
            snippet: "(${node}, ${})".into(),
            description: "wrap as tuple (array) expression".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "let".into(),
            label_detail: Some(".let".into()),
            snippet: "let ${_} = ${node}".into(),
            description: "wrap as let expression".into(),
            parsed_snippet: OnceLock::new(),
        },
        PostfixSnippet {
            scope: PostfixSnippetScope::Value,
            mode: eco_vec![InterpretMode::Code, InterpretMode::Markup],
            label: "in".into(),
            label_detail: Some(".in".into()),
            snippet: "${_} in ${node}".into(),
            description: "wrap with in expression".into(),
            parsed_snippet: OnceLock::new(),
        },
    ]
});
