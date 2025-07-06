use std::collections::HashMap;

use ecow::EcoString;
use lsp_types::{
    ChangeAnnotation, ChangeAnnotationIdentifier, CodeActionDisabled, CodeActionKind, Command,
    Diagnostic, InsertTextFormat, OneOf, OptionalVersionedTextDocumentIdentifier, ResourceOp, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::LspRange;
use crate::completion::EcoTextEdit;

/// A textual edit applicable to a text document.
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoSnippetTextEdit {
    /// The text edit
    #[serde(flatten)]
    edit: EcoTextEdit,
    /// The format of the insert text. The format applies to both the
    insert_text_format: Option<InsertTextFormat>,
}

impl EcoSnippetTextEdit {
    /// Creates a new plain text edit.
    pub fn new_plain(range: LspRange, new_text: EcoString) -> Self {
        Self {
            edit: EcoTextEdit::new(range, new_text),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        }
    }

    /// Creates a new snippet text edit.
    pub fn new(range: LspRange, new_text: EcoString) -> Self {
        Self {
            edit: EcoTextEdit::new(range, new_text),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
        }
    }
}

/// A special text edit with an additional change annotation.
///
/// @since 3.16.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoAnnotatedTextEdit {
    /// The base text edit.
    #[serde(flatten)]
    pub text_edit: EcoSnippetTextEdit,

    /// The actual annotation
    pub annotation_id: ChangeAnnotationIdentifier,
}

/// Describes textual changes on a single text document. The text document is
/// referred to as a `OptionalVersionedTextDocumentIdentifier` to allow clients
/// to check the text document version before an edit is applied. A
/// `TextDocumentEdit` describes all changes on a version Si and after they are
/// applied move the document to version Si+1. So the creator of a
/// `TextDocumentEdit` doesn't need to sort the array or do any kind of
/// ordering. However the edits must be non overlapping.
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoTextDocumentEdit {
    /// The text document to change.
    pub text_document: OptionalVersionedTextDocumentIdentifier,

    /// The edits to be applied.
    ///
    /// @since 3.16.0 - support for AnnotatedTextEdit. This is guarded by the
    /// client capability `workspace.workspaceEdit.changeAnnotationSupport`
    pub edits: Vec<OneOf<EcoSnippetTextEdit, EcoAnnotatedTextEdit>>,
}

/// A code action represents to a single or multiple editor behaviors that can
/// be triggered in a text document.
#[derive(Debug, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    /// A short, human-readable, title for this code action.
    pub title: String,

    /// The kind of the code action.
    /// Used to filter code actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<CodeActionKind>,

    /// The diagnostics that this code action resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<Diagnostic>>,

    /// The workspace edit this code action performs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<EcoWorkspaceEdit>,

    /// A command this code action executes. If a code action
    /// provides an edit and a command, first the edit is
    /// executed and then the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,

    /// Marks this as a preferred action. Preferred actions are used by the
    /// `auto fix` command and can be targeted by keybindings.
    /// A quick fix should be marked preferred if it properly addresses the
    /// underlying error. A refactoring should be marked preferred if it is
    /// the most reasonable choice of actions to take.
    ///
    /// @since 3.15.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_preferred: Option<bool>,

    /// Marks that the code action cannot currently be applied.
    ///
    /// Clients should follow the following guidelines regarding disabled code
    /// actions:
    ///
    /// - Disabled code actions are not shown in automatic [lightbulb](https://code.visualstudio.com/docs/editor/editingevolved#_code-action)
    ///   code action menu.
    ///
    /// - Disabled actions are shown as faded out in the code action menu when
    ///   the user request a more specific type of code action, such as
    ///   refactorings.
    ///
    /// - If the user has a keybinding that auto applies a code action and only
    ///   a disabled code actions are returned, the client should show the user
    ///   an error message with `reason` in the editor.
    ///
    /// @since 3.16.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<CodeActionDisabled>,

    /// A data entry field that is preserved on a code action between
    /// a `textDocument/codeAction` and a `codeAction/resolve` request.
    ///
    /// @since 3.16.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A workspace edit represents changes to many resources managed in the
/// workspace. The edit should either provide `changes` or `documentChanges`.
/// If the client can handle versioned document edits and if `documentChanges`
/// are present, the latter are preferred over `changes`.
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoWorkspaceEdit {
    /// Holds changes to existing resources.
    #[serde(with = "url_map")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub changes: Option<HashMap<Url, Vec<EcoSnippetTextEdit>>>,

    /// Depending on the client capability
    /// `workspace.workspaceEdit.resourceOperations` document changes
    /// are either an array of `TextDocumentEdit`s to express changes to n
    /// different text documents where each text document edit addresses a
    /// specific version of a text document. Or it can contain
    /// above `TextDocumentEdit`s mixed with create, rename and delete file /
    /// folder operations.
    ///
    /// Whether a client supports versioned document edits is expressed via
    /// `workspace.workspaceEdit.documentChanges` client capability.
    ///
    /// If a client neither supports `documentChanges` nor
    /// `workspace.workspaceEdit.resourceOperations` then only plain
    /// `TextEdit`s using the `changes` property are supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_changes: Option<EcoDocumentChanges>,

    /// A map of change annotations that can be referenced in
    /// `AnnotatedTextEdit`s or create, rename and delete file / folder
    /// operations.
    ///
    /// Whether clients honor this property depends on the client capability
    /// `workspace.changeAnnotationSupport`.
    ///
    /// @since 3.16.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_annotations: Option<HashMap<ChangeAnnotationIdentifier, ChangeAnnotation>>,
}

/// The `documentChanges` property of a `WorkspaceEdit` can contain
/// `TextDocumentEdit`s to express changes to n different text documents
/// where each text document edit addresses a specific version of a text
/// document. Or it can contain create, rename and delete file / folder
/// operations.
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EcoDocumentChanges {
    /// Text document edits
    Edits(Vec<EcoTextDocumentEdit>),
    /// Resource operations
    Operations(Vec<EcoDocumentChangeOperation>),
}

/// A resource operation represents changes to existing resources or
/// creation of new resources. The operation can be a create, rename or
/// delete operation.
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(untagged, rename_all = "lowercase")]
pub enum EcoDocumentChangeOperation {
    /// A resource operation.
    Op(ResourceOp),
    /// A text document edit.
    Edit(EcoTextDocumentEdit),
}

mod url_map {
    use std::marker::PhantomData;
    use std::{collections::HashMap, fmt};

    use lsp_types::Url;
    use serde::de;

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<Option<HashMap<Url, V>>, D::Error>
    where
        D: serde::Deserializer<'de>,
        V: de::DeserializeOwned,
    {
        struct UrlMapVisitor<V> {
            _marker: PhantomData<V>,
        }

        impl<V: de::DeserializeOwned> Default for UrlMapVisitor<V> {
            fn default() -> Self {
                Self {
                    _marker: PhantomData,
                }
            }
        }
        impl<'de, V: de::DeserializeOwned> de::Visitor<'de> for UrlMapVisitor<V> {
            type Value = HashMap<Url, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("map")
            }

            fn visit_map<M>(self, mut visitor: M) -> Result<Self::Value, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let mut values = HashMap::with_capacity(visitor.size_hint().unwrap_or(0));

                // While there are entries remaining in the input, add them
                // into our map.
                while let Some((key, value)) = visitor.next_entry::<Url, _>()? {
                    values.insert(key, value);
                }

                Ok(values)
            }
        }

        struct OptionUrlMapVisitor<V> {
            _marker: PhantomData<V>,
        }
        impl<V: de::DeserializeOwned> Default for OptionUrlMapVisitor<V> {
            fn default() -> Self {
                Self {
                    _marker: PhantomData,
                }
            }
        }
        impl<'de, V: de::DeserializeOwned> de::Visitor<'de> for OptionUrlMapVisitor<V> {
            type Value = Option<HashMap<Url, V>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("option")
            }

            #[inline]
            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }

            #[inline]
            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }

            #[inline]
            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer
                    .deserialize_map(UrlMapVisitor::<V>::default())
                    .map(Some)
            }
        }

        // Instantiate our Visitor and ask the Deserializer to drive
        // it over the input data, resulting in an instance of MyMap.
        deserializer.deserialize_option(OptionUrlMapVisitor::default())
    }

    pub fn serialize<S, V>(
        changes: &Option<HashMap<Url, V>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
        V: serde::Serialize,
    {
        use serde::ser::SerializeMap;

        match *changes {
            Some(ref changes) => {
                let mut map = serializer.serialize_map(Some(changes.len()))?;
                for (k, v) in changes {
                    map.serialize_entry(k.as_str(), v)?;
                }
                map.end()
            }
            None => serializer.serialize_none(),
        }
    }
}
