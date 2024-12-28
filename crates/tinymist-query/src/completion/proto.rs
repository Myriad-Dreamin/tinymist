use ecow::EcoString;
use lsp_types::InsertTextFormat;
use serde::{Deserialize, Serialize};

use crate::ty::Interned;

use super::LspRange;

/// A kind of item that can be completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionKind {
    /// A syntactical structure.
    Syntax,
    /// A function.
    Func,
    /// A type.
    Type,
    /// A function parameter.
    Param,
    /// A field.
    Field,
    /// A constant.
    #[default]
    Constant,
    /// A reference.
    Reference,
    /// A symbol.
    Symbol(char),
    /// A variable.
    Variable,
    /// A module.
    Module,
    /// A file.
    File,
    /// A folder.
    Folder,
}

impl From<CompletionKind> for lsp_types::CompletionItemKind {
    fn from(value: CompletionKind) -> Self {
        match value {
            CompletionKind::Syntax => Self::SNIPPET,
            CompletionKind::Func => Self::FUNCTION,
            CompletionKind::Param => Self::VARIABLE,
            CompletionKind::Field => Self::FIELD,
            CompletionKind::Variable => Self::VARIABLE,
            CompletionKind::Constant => Self::CONSTANT,
            CompletionKind::Reference => Self::REFERENCE,
            CompletionKind::Symbol(_) => Self::FIELD,
            CompletionKind::Type => Self::CLASS,
            CompletionKind::Module => Self::MODULE,
            CompletionKind::File => Self::FILE,
            CompletionKind::Folder => Self::FOLDER,
        }
    }
}

impl serde::Serialize for CompletionKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <Self as Into<lsp_types::CompletionItemKind>>::into(*self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for CompletionKind {
    fn deserialize<D>(deserializer: D) -> Result<CompletionKind, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let kind = lsp_types::CompletionItemKind::deserialize(deserializer)?;
        Ok(match kind {
            lsp_types::CompletionItemKind::SNIPPET => CompletionKind::Syntax,
            lsp_types::CompletionItemKind::FUNCTION => CompletionKind::Func,
            lsp_types::CompletionItemKind::VARIABLE => CompletionKind::Param,
            lsp_types::CompletionItemKind::FIELD => CompletionKind::Field,
            lsp_types::CompletionItemKind::CONSTANT => CompletionKind::Constant,
            lsp_types::CompletionItemKind::REFERENCE => CompletionKind::Reference,
            lsp_types::CompletionItemKind::CLASS => CompletionKind::Type,
            lsp_types::CompletionItemKind::MODULE => CompletionKind::Module,
            lsp_types::CompletionItemKind::FILE => CompletionKind::File,
            lsp_types::CompletionItemKind::FOLDER => CompletionKind::Folder,
            _ => CompletionKind::Variable,
        })
    }
}

/// An autocompletion option.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Completion {
    /// The kind of item this completes to.
    pub kind: CompletionKind,
    /// The label the completion is shown with.
    pub label: EcoString,
    /// The label the completion is shown with.
    pub label_details: Option<EcoString>,
    /// The label the completion is shown with.
    pub sort_text: Option<EcoString>,
    /// The composed text used for filtering.
    pub filter_text: Option<EcoString>,
    /// The completed version of the input, possibly described with snippet
    /// syntax like `${lhs} + ${rhs}`.
    ///
    /// Should default to the `label` if `None`.
    pub apply: Option<EcoString>,
    /// An optional short description, at most one sentence.
    pub detail: Option<EcoString>,
    /// An optional array of additional text edits that are applied when
    /// selecting this completion. Edits must not overlap with the main edit
    /// nor with themselves.
    pub additional_text_edits: Option<Vec<EcoTextEdit>>,
    /// An optional command to run when the completion is selected.
    pub command: Option<LspCompletionCommand>,
}

/// Represents a collection of [completion items](#CompletionItem) to be
/// presented in the editor.
#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    /// This list it not complete. Further typing should result in recomputing
    /// this list.
    pub is_incomplete: bool,

    /// The completion items.
    pub items: Vec<CompletionItem>,
}

/// Additional details for a completion item label.
///
/// @since 3.17.0
#[derive(Debug, PartialEq, Default, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItemLabelDetails {
    /// An optional string which is rendered less prominently directly after
    /// {@link CompletionItemLabel.label label}, without any spacing. Should be
    /// used for function signatures or type annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<EcoString>,

    /// An optional string which is rendered less prominently after
    /// {@link CompletionItemLabel.detail}. Should be used for fully qualified
    /// names or file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<EcoString>,
}

impl From<EcoString> for CompletionItemLabelDetails {
    fn from(description: EcoString) -> Self {
        Self {
            detail: None,
            description: Some(description),
        }
    }
}

/// A textual edit applicable to a text document.
///
/// If n `EcoTextEdit`s are applied to a text document all text edits describe
/// changes to the initial document version. Execution wise text edits should
/// applied from the bottom to the top of the text document. Overlapping text
/// edits are not supported.
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoTextEdit {
    /// The range of the text document to be manipulated. To insert
    /// text into a document create a range where start === end.
    pub range: LspRange,
    /// The string to be inserted. For delete operations use an
    /// empty string.
    pub new_text: EcoString,
}

impl EcoTextEdit {
    pub fn new(range: LspRange, new_text: EcoString) -> EcoTextEdit {
        EcoTextEdit { range, new_text }
    }
}

/// Represents a completion item.
#[derive(Debug, PartialEq, Default, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    /// The label of this completion item. By default
    /// also the text that is inserted when selecting
    /// this completion.
    pub label: EcoString,

    /// Additional details for the label
    ///
    /// @since 3.17.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_details: Option<CompletionItemLabelDetails>,

    /// The kind of this completion item. Based of the kind
    /// an icon is chosen by the editor.
    pub kind: CompletionKind,

    /// A human-readable string with additional information
    /// about this item, like type or symbol information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<EcoString>,

    /// A string that should be used when comparing this item
    /// with other items. When `falsy` the label is used
    /// as the sort text for this item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_text: Option<EcoString>,

    /// A string that should be used when filtering a set of
    /// completion items. When `falsy` the label is used as the
    /// filter text for this item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_text: Option<EcoString>,

    /// A string that should be inserted into a document when selecting
    /// this completion. When `falsy` the label is used as the insert text
    /// for this item.
    ///
    /// The `insertText` is subject to interpretation by the client side.
    /// Some tools might not take the string literally. For example
    /// VS Code when code complete is requested in this example
    /// `con<cursor position>` and a completion item with an `insertText` of
    /// `console` is provided it will only insert `sole`. Therefore it is
    /// recommended to use `textEdit` instead since it avoids additional client
    /// side interpretation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<EcoString>,

    /// The format of the insert text. The format applies to both the
    /// `insertText` property and the `newText` property of a provided
    /// `textEdit`. If omitted defaults to `InsertTextFormat.PlainText`.
    ///
    /// @since 3.16.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text_format: Option<InsertTextFormat>,

    /// An edit which is applied to a document when selecting
    /// this completion. When an edit is provided the value of
    /// insertText is ignored.
    ///
    /// Most editors support two different operation when accepting a completion
    /// item. One is to insert a completion text and the other is to replace an
    /// existing text with a completion text. Since this can usually not
    /// predetermined by a server it can report both ranges. Clients need to
    /// signal support for `InsertReplaceEdits` via the
    /// `textDocument.completion.insertReplaceSupport` client capability
    /// property.
    ///
    /// *Note 1:* The text edit's range as well as both ranges from a insert
    /// replace edit must be a [single line] and they must contain the
    /// position at which completion has been requested. *Note 2:* If an
    /// `InsertReplaceEdit` is returned the edit's insert range must be a prefix
    /// of the edit's replace range, that means it must be contained and
    /// starting at the same position.
    ///
    /// @since 3.16.0 additional type `InsertReplaceEdit`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_edit: Option<EcoTextEdit>,

    /// An optional array of additional text edits that are applied when
    /// selecting this completion. Edits must not overlap with the main edit
    /// nor with themselves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_text_edits: Option<Vec<EcoTextEdit>>,

    /// An optional command that is executed *after* inserting this completion.
    /// *Note* that additional modifications to the current document should
    /// be described with the additionalTextEdits-property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<LspCompletionCommand>,
}

/// Represents a reference to a command. Provides a title which will be used to
/// represent a command in the UI. Commands are identified by a string
/// identifier. The recommended way to handle commands is to implement
/// their execution on the server side if the client and server provides the
/// corresponding capabilities. Alternatively the tool extension code could
/// handle the command. The protocol currently doesn’t specify a set of
/// well-known commands.
#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize)]
pub struct LspCompletionCommand {
    /// The title of command.
    pub title: EcoString,
    /// The identifier of the actual command handler.
    pub command: Interned<str>,
}

impl From<Interned<str>> for LspCompletionCommand {
    fn from(command: Interned<str>) -> Self {
        Self {
            title: EcoString::default(),
            command,
        }
    }
}
