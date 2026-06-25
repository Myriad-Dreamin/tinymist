//! Offline query SCIP indexes.

use serde::Serialize;
use tinymist_std::error::{WithContextUntyped, prelude::*};
use tinymist_std::hash::{FxHashMap, FxHashSet};

use crate::{
    CompilerQueryRequest, CompilerQueryResponse, GotoDefinitionRequest, HoverRequest, path_to_url,
    url_to_path,
};

use lsp_types::{
    GotoDefinitionResponse, Hover, HoverContents, LocationLink, MarkedString, Position, Range, Url,
};
use protobuf::{Enum, Message};

use super::scip_utils::{ScipParamGroup, merge_symbol_information};

/// The context for querying a SCIP index.
#[derive(Default)]
pub struct ScipQueryCtx {
    documents_by_uri: FxHashMap<Url, ScipDocumentQuery>,
    documents_by_path: FxHashMap<String, Url>,
    definitions: FxHashMap<String, Vec<ScipDefinition>>,
    public_symbols_by_path: FxHashMap<String, Vec<ScipPublicSymbol>>,
    symbol_hovers: FxHashMap<String, Hover>,
}

/// A public package symbol exported by a module.
#[derive(Debug, Clone, Serialize)]
pub struct ScipPublicSymbol {
    /// SCIP symbol.
    pub symbol: String,
    /// Display name.
    pub name: String,
    /// Package docs definition kind.
    pub kind: String,
}

/// A token on a source page that has index-backed interactions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScipSourceToken {
    /// Token range in LSP UTF-16 positions.
    pub range: Range,
    /// Hover information for the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<Hover>,
    /// First definition target for the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<LocationLink>,
}

#[derive(Debug, Clone)]
struct ScipDocumentQuery {
    uri: Url,
    occurrences: Vec<ScipOccurrence>,
}

#[derive(Debug, Clone)]
struct ScipOccurrence {
    range: Range,
    symbol: String,
    is_definition: bool,
    hover: Option<Hover>,
}

#[derive(Debug, Clone)]
struct ScipDefinition {
    uri: Url,
    range: Range,
}

impl ScipQueryCtx {
    /// Reads the index from SCIP protobuf bytes.
    pub fn read(db: &[u8]) -> Result<Self> {
        let index =
            scip::types::Index::parse_from_bytes(db).context_ut("failed to parse SCIP index")?;
        Ok(Self::build_query_tables(index))
    }

    fn build_query_tables(index: scip::types::Index) -> Self {
        let project_root = index
            .metadata
            .as_ref()
            .and_then(|metadata| Url::parse(&metadata.project_root).ok());
        let symbol_infos = collect_symbol_infos(&index);
        let mut this = Self {
            symbol_hovers: collect_symbol_hovers(&symbol_infos),
            ..Default::default()
        };

        for document in index.documents {
            let relative_path = document.relative_path.clone();
            let Some(uri) = scip_document_uri(project_root.as_ref(), &document.relative_path)
            else {
                continue;
            };
            let public_symbols = scip_public_symbols(&document, &symbol_infos);
            let mut occurrences = Vec::with_capacity(document.occurrences.len());
            for occurrence in document.occurrences {
                let Some(range) = scip_range_to_lsp(&occurrence.range) else {
                    continue;
                };
                let is_definition =
                    occurrence.symbol_roles & scip::types::SymbolRole::Definition.value() != 0;
                let hover = hover_from_scip_docs(&occurrence.override_documentation);
                let symbol = occurrence.symbol;

                if !symbol.is_empty() && is_definition {
                    this.definitions
                        .entry(symbol.clone())
                        .or_default()
                        .push(ScipDefinition {
                            uri: uri.clone(),
                            range,
                        });
                }

                occurrences.push(ScipOccurrence {
                    range,
                    symbol,
                    is_definition,
                    hover,
                });
            }

            if !public_symbols.is_empty() {
                this.public_symbols_by_path
                    .insert(relative_path.clone(), public_symbols);
            }
            this.documents_by_path.insert(relative_path, uri.clone());
            this.documents_by_uri
                .insert(uri.clone(), ScipDocumentQuery { uri, occurrences });
        }

        this
    }

    /// Requests the index for a compiler query.
    pub fn request(&mut self, request: CompilerQueryRequest) -> Option<CompilerQueryResponse> {
        match request {
            CompilerQueryRequest::Hover(request) => {
                Some(CompilerQueryResponse::Hover(self.hover(request)))
            }
            CompilerQueryRequest::HoverSymbol(symbol) => {
                Some(CompilerQueryResponse::Hover(self.hover_symbol(&symbol)))
            }
            CompilerQueryRequest::GotoDefinition(request) => Some(
                CompilerQueryResponse::GotoDefinition(self.goto_definition(request)),
            ),
            CompilerQueryRequest::GotoDefinitionSymbol(symbol) => Some(
                CompilerQueryResponse::GotoDefinition(self.goto_definition_symbol(&symbol)),
            ),
            _ => None,
        }
    }

    fn hover(&self, request: HoverRequest) -> Option<Hover> {
        let uri = path_to_url(&request.path).ok()?;
        let document = self.documents_by_uri.get(&uri)?;
        let occurrence = find_scip_occurrence(document, request.position)?;
        let mut hover = occurrence
            .hover
            .clone()
            .or_else(|| self.symbol_hovers.get(&occurrence.symbol).cloned())?;
        hover.range.get_or_insert(occurrence.range);
        Some(hover)
    }

    /// Requests the hover information for a SCIP symbol.
    pub fn hover_symbol(&self, symbol: &str) -> Option<Hover> {
        self.symbol_hovers.get(symbol).cloned()
    }

    /// Requests the public symbols exported by a SCIP document path.
    pub fn public_symbols(&self, path: &str) -> Vec<ScipPublicSymbol> {
        self.public_symbols_by_path
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    /// Requests source-page tokens that have hover or definition information.
    pub fn source_tokens(&self, path: &str) -> Vec<ScipSourceToken> {
        let Some(uri) = self.documents_by_path.get(path) else {
            return Vec::new();
        };
        let Some(document) = self.documents_by_uri.get(uri) else {
            return Vec::new();
        };

        document
            .occurrences
            .iter()
            .filter(|occurrence| !occurrence.symbol.is_empty())
            .filter_map(|occurrence| {
                let mut hover = occurrence
                    .hover
                    .clone()
                    .or_else(|| self.symbol_hovers.get(&occurrence.symbol).cloned());
                if let Some(hover) = &mut hover {
                    hover.range.get_or_insert(occurrence.range);
                }

                let definition = self
                    .definition_links_for_occurrence(document, occurrence)
                    .into_iter()
                    .next();

                if hover.is_none() && definition.is_none() {
                    return None;
                }

                Some(ScipSourceToken {
                    range: occurrence.range,
                    hover,
                    definition,
                })
            })
            .collect()
    }

    fn goto_definition(&self, request: GotoDefinitionRequest) -> Option<GotoDefinitionResponse> {
        let uri = path_to_url(&request.path).ok()?;
        let document = self.documents_by_uri.get(&uri)?;
        let occurrence = find_scip_occurrence(document, request.position)?;
        let links = self.definition_links_for_occurrence(document, occurrence);

        if links.is_empty() {
            None
        } else {
            Some(GotoDefinitionResponse::Link(links))
        }
    }

    fn definition_links_for_occurrence(
        &self,
        document: &ScipDocumentQuery,
        occurrence: &ScipOccurrence,
    ) -> Vec<LocationLink> {
        let origin_selection_range = occurrence.range;
        if occurrence.is_definition {
            return vec![LocationLink {
                origin_selection_range: Some(origin_selection_range),
                target_uri: document.uri.clone(),
                target_range: occurrence.range,
                target_selection_range: occurrence.range,
            }];
        }

        self.definitions
            .get(&occurrence.symbol)
            .into_iter()
            .flatten()
            .map(|definition| LocationLink {
                origin_selection_range: Some(origin_selection_range),
                target_uri: definition.uri.clone(),
                target_range: definition.range,
                target_selection_range: definition.range,
            })
            .collect()
    }

    /// Requests the definition for a SCIP symbol.
    pub fn goto_definition_symbol(&self, symbol: &str) -> Option<GotoDefinitionResponse> {
        let links = self
            .definitions
            .get(symbol)?
            .iter()
            .map(|definition| LocationLink {
                origin_selection_range: None,
                target_uri: definition.uri.clone(),
                target_range: definition.range,
                target_selection_range: definition.range,
            })
            .collect::<Vec<_>>();

        if links.is_empty() {
            None
        } else {
            Some(GotoDefinitionResponse::Link(links))
        }
    }
}

fn collect_symbol_infos(
    index: &scip::types::Index,
) -> FxHashMap<String, scip::types::SymbolInformation> {
    let mut symbol_infos = FxHashMap::default();
    for symbol in index.external_symbols.iter().chain(
        index
            .documents
            .iter()
            .flat_map(|document| document.symbols.iter()),
    ) {
        if let Some(current) = symbol_infos.get_mut(&symbol.symbol) {
            merge_symbol_information(current, symbol.clone());
        } else {
            symbol_infos.insert(symbol.symbol.clone(), symbol.clone());
        }
    }
    symbol_infos
}

fn collect_symbol_hovers(
    symbol_infos: &FxHashMap<String, scip::types::SymbolInformation>,
) -> FxHashMap<String, Hover> {
    symbol_infos
        .values()
        .filter_map(|symbol| {
            hover_from_scip_symbol(symbol, symbol_infos).map(|hover| (symbol.symbol.clone(), hover))
        })
        .collect()
}

fn scip_public_symbols(
    document: &scip::types::Document,
    symbol_infos: &FxHashMap<String, scip::types::SymbolInformation>,
) -> Vec<ScipPublicSymbol> {
    let mut symbols = Vec::new();
    let mut seen = FxHashSet::default();

    for symbol in &document.symbols {
        if scip_package_docs_kind(symbol) != "module" {
            continue;
        }

        for relationship in &symbol.relationships {
            if !relationship.is_definition || !seen.insert(relationship.symbol.clone()) {
                continue;
            }

            let Some(info) = symbol_infos.get(&relationship.symbol) else {
                continue;
            };
            symbols.push(ScipPublicSymbol {
                symbol: relationship.symbol.clone(),
                name: scip_symbol_display_name(info, &relationship.symbol),
                kind: scip_package_docs_kind(info).to_owned(),
            });
        }
    }

    symbols
}

fn scip_symbol_display_name(info: &scip::types::SymbolInformation, symbol: &str) -> String {
    if !info.display_name.is_empty() {
        return info.display_name.clone();
    }

    scip::symbol::parse_symbol(symbol)
        .ok()
        .and_then(|symbol| {
            symbol
                .descriptors
                .last()
                .map(|descriptor| descriptor.name.clone())
        })
        .unwrap_or_default()
}

fn scip_package_docs_kind(info: &scip::types::SymbolInformation) -> &'static str {
    match info.kind.enum_value().ok() {
        Some(scip::types::symbol_information::Kind::Module) => "module",
        Some(scip::types::symbol_information::Kind::Function) => "function",
        Some(scip::types::symbol_information::Kind::Constant) => "constant",
        Some(scip::types::symbol_information::Kind::Variable) => "variable",
        Some(scip::types::symbol_information::Kind::Parameter) => "parameter",
        _ => "unknown",
    }
}

fn find_scip_occurrence(
    document: &ScipDocumentQuery,
    position: Position,
) -> Option<&ScipOccurrence> {
    document
        .occurrences
        .iter()
        .filter(|occurrence| contains_position(&occurrence.range, position))
        .min_by_key(|occurrence| range_len_key(&occurrence.range))
}

fn scip_document_uri(project_root: Option<&Url>, relative_path: &str) -> Option<Url> {
    if let Some(project_root) = project_root
        && project_root.scheme() == "file"
    {
        let mut path = url_to_path(project_root);
        path.push(relative_path);
        if let Ok(uri) = path_to_url(&path) {
            return Some(uri);
        }
    }

    if let Some(project_root) = project_root
        && let Ok(uri) = project_root.join(relative_path)
    {
        return Some(uri);
    }

    let path = std::path::Path::new(relative_path);
    if path.is_absolute() {
        path_to_url(path).ok()
    } else {
        None
    }
}

fn scip_range_to_lsp(range: &[i32]) -> Option<Range> {
    let line = as_u32(*range.first()?)?;
    let start_character = as_u32(*range.get(1)?)?;
    let (end_line, end_character) = match range.len() {
        3 => (line, as_u32(range[2])?),
        4 => (as_u32(range[2])?, as_u32(range[3])?),
        _ => return None,
    };

    Some(Range {
        start: Position {
            line,
            character: start_character,
        },
        end: Position {
            line: end_line,
            character: end_character,
        },
    })
}

fn as_u32(value: i32) -> Option<u32> {
    u32::try_from(value).ok()
}

fn hover_from_scip_symbol(
    symbol: &scip::types::SymbolInformation,
    symbol_infos: &FxHashMap<String, scip::types::SymbolInformation>,
) -> Option<Hover> {
    if let Some(signature) = symbol.signature_documentation.as_ref()
        && !signature.text.trim().is_empty()
    {
        return hover_from_scip_signature_symbol(symbol, signature, symbol_infos);
    }

    hover_from_scip_docs(&symbol.documentation)
}

fn hover_from_scip_signature_symbol(
    symbol: &scip::types::SymbolInformation,
    signature: &scip::types::Signature,
    symbol_infos: &FxHashMap<String, scip::types::SymbolInformation>,
) -> Option<Hover> {
    let mut docs = Vec::new();
    let language = if signature.language.trim().is_empty() {
        "typc"
    } else {
        signature.language.as_str()
    };
    docs.push(format!("```{language}\n{}\n```", signature.text));
    docs.extend(symbol.documentation.iter().cloned());

    let params = scip_signature_params(signature, symbol_infos);
    extend_scip_param_section(
        &mut docs,
        "# Positional Parameters",
        "positional",
        &params.positional,
    );
    extend_scip_param_section(&mut docs, "# Rest Parameters", "rest", &params.rest);
    extend_scip_param_section(&mut docs, "# Named Parameters", "named", &params.named);

    let markdown = docs
        .into_iter()
        .filter(|doc| !doc.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if markdown.trim().is_empty() {
        return None;
    }

    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String(markdown)),
        range: None,
    })
}

#[derive(Default)]
struct ScipSignatureParams {
    positional: Vec<ScipSignatureParam>,
    rest: Vec<ScipSignatureParam>,
    named: Vec<ScipSignatureParam>,
}

struct ScipSignatureParam {
    name: String,
    documentation: Vec<String>,
}

fn scip_signature_params(
    signature: &scip::types::Signature,
    symbol_infos: &FxHashMap<String, scip::types::SymbolInformation>,
) -> ScipSignatureParams {
    let mut occurrences = signature.occurrences.iter().collect::<Vec<_>>();
    occurrences.sort_by(|left, right| {
        left.range
            .cmp(&right.range)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });

    let mut seen = FxHashSet::default();
    let mut params = ScipSignatureParams::default();
    for occurrence in occurrences {
        if !seen.insert(occurrence.symbol.clone()) {
            continue;
        }
        let Some(group) = scip_parameter_group(&occurrence.symbol) else {
            continue;
        };
        let Some(info) = symbol_infos.get(&occurrence.symbol) else {
            continue;
        };
        let param = ScipSignatureParam {
            name: if info.display_name.is_empty() {
                scip_parameter_name(&occurrence.symbol).unwrap_or_default()
            } else {
                info.display_name.clone()
            },
            documentation: info.documentation.clone(),
        };

        match group {
            ScipParamGroup::Positional => params.positional.push(param),
            ScipParamGroup::Rest => params.rest.push(param),
            ScipParamGroup::Named => params.named.push(param),
        }
    }

    params
}

fn scip_parameter_group(symbol: &str) -> Option<ScipParamGroup> {
    let symbol = scip::symbol::parse_symbol(symbol).ok()?;
    let mut saw_parameter = false;
    for descriptor in symbol.descriptors.iter().rev() {
        match descriptor.suffix.enum_value().ok()? {
            scip::types::descriptor::Suffix::Parameter => saw_parameter = true,
            scip::types::descriptor::Suffix::Meta if saw_parameter => {
                return ScipParamGroup::from_descriptor(&descriptor.name);
            }
            _ if saw_parameter => return None,
            _ => {}
        }
    }

    None
}

fn scip_parameter_name(symbol: &str) -> Option<String> {
    let symbol = scip::symbol::parse_symbol(symbol).ok()?;
    symbol.descriptors.iter().rev().find_map(|descriptor| {
        matches!(
            descriptor.suffix.enum_value().ok()?,
            scip::types::descriptor::Suffix::Parameter
        )
        .then(|| descriptor.name.clone())
    })
}

fn extend_scip_param_section(
    docs: &mut Vec<String>,
    title: &str,
    kind: &str,
    params: &[ScipSignatureParam],
) {
    if params.is_empty() {
        return;
    }

    let mut section = title.to_owned();
    for (idx, param) in params.iter().enumerate() {
        let heading = if idx == 0 {
            param.name.clone()
        } else {
            format!("{} ({kind})", param.name)
        };
        section.push_str("\n\n## ");
        section.push_str(&heading);

        let body = param
            .documentation
            .iter()
            .filter(|doc| !doc.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        if !body.trim().is_empty() {
            section.push_str("\n\n");
            section.push_str(&body);
        }
    }
    docs.push(section);
}

fn hover_from_scip_docs(docs: &[String]) -> Option<Hover> {
    let docs = docs
        .iter()
        .filter(|doc| !doc.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    if docs.is_empty() {
        return None;
    }

    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String(docs.join("\n\n---\n\n"))),
        range: None,
    })
}

fn contains_position(range: &Range, position: Position) -> bool {
    position_after_or_eq(position, range.start) && position_before(position, range.end)
}

fn position_after_or_eq(a: Position, b: Position) -> bool {
    (a.line, a.character) >= (b.line, b.character)
}

fn position_before(a: Position, b: Position) -> bool {
    (a.line, a.character) < (b.line, b.character)
}

fn range_len_key(range: &Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lsp_types::{HoverContents, MarkedString};
    use protobuf::{Enum, EnumOrUnknown, Message, MessageField};
    use scip::types::{
        Document as ScipDocument, Index as ScipIndex, Metadata as ScipMetadata,
        Occurrence as ScipOccurrence, PositionEncoding as ScipPositionEncoding,
        Relationship as ScipRelationship, Signature as ScipSignature,
        SymbolInformation as ScipSymbolInformation, SymbolRole as ScipSymbolRole,
        TextEncoding as ScipTextEncoding, ToolInfo as ScipToolInfo, symbol_information,
    };

    use super::*;

    #[test]
    fn scip_hover_and_definition() {
        let path = fixture_path();
        let symbol = "local value";
        let bytes = test_index(vec![main_document(
            vec![
                definition_occurrence(vec![0, 0, 4], symbol),
                occurrence(vec![1, 0, 4], symbol),
            ],
            vec![symbol_info(symbol, "value", &["hello"])],
        )]);
        let mut index = ScipQueryCtx::read(&bytes).unwrap();

        let hover = index.request(CompilerQueryRequest::Hover(HoverRequest {
            path: path.clone(),
            position: Position {
                line: 1,
                character: 1,
            },
        }));
        let Some(CompilerQueryResponse::Hover(Some(hover))) = hover else {
            panic!("expected SCIP hover response");
        };
        assert_eq!(
            hover.contents,
            HoverContents::Scalar(MarkedString::String("hello".to_owned()))
        );

        let definition = index.request(CompilerQueryRequest::GotoDefinition(
            GotoDefinitionRequest {
                path,
                position: Position {
                    line: 1,
                    character: 1,
                },
            },
        ));
        let Some(CompilerQueryResponse::GotoDefinition(Some(GotoDefinitionResponse::Link(links)))) =
            definition
        else {
            panic!("expected SCIP definition response");
        };
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].target_selection_range,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            }
        );
    }

    #[test]
    fn scip_signature_hover_uses_parameter_symbols() {
        let path = fixture_path();
        let symbol = "typst tinymist workspace . main.typ/L0_C0:foo().";
        let param_symbol = "typst tinymist workspace . main.typ/L0_C0:foo().positional:(arg)";
        let mut function = symbol_info(symbol, "foo", &["Function docs."]);
        function.signature_documentation = MessageField::some(ScipSignature {
            language: "typc".to_owned(),
            text: "let foo(\n  arg: int,\n) = none;".to_owned(),
            occurrences: vec![definition_occurrence(vec![1, 2, 5], param_symbol)],
            ..Default::default()
        });
        let mut parameter = symbol_info(
            param_symbol,
            "arg",
            &["```typc\ntype: int\n```", "Arg docs."],
        );
        parameter.enclosing_symbol = symbol.to_owned();
        let bytes = test_index(vec![main_document(
            vec![occurrence(vec![1, 0, 3], symbol)],
            vec![function, parameter],
        )]);
        let mut index = ScipQueryCtx::read(&bytes).unwrap();

        let hover = index.request(CompilerQueryRequest::Hover(HoverRequest {
            path,
            position: Position {
                line: 1,
                character: 1,
            },
        }));
        let Some(CompilerQueryResponse::Hover(Some(hover))) = hover else {
            panic!("expected SCIP hover response");
        };
        assert_eq!(
            hover.contents,
            HoverContents::Scalar(MarkedString::String(
                "```typc\nlet foo(\n  arg: int,\n) = none;\n```\n\nFunction docs.\n\n# Positional Parameters\n\n## arg\n\n```typc\ntype: int\n```\n\nArg docs.".to_owned()
            ))
        );
    }

    #[test]
    fn scip_public_symbols_from_module_document() {
        let module_symbol = "typst tinymist workspace . main.typ/1:main/";
        let function_symbol = "typst tinymist workspace . main.typ/L0_C0:foo().";
        let mut module =
            typed_symbol_info(module_symbol, "main", symbol_information::Kind::Module, &[]);
        module.relationships = vec![public_relationship(function_symbol)];
        let function = typed_symbol_info(
            function_symbol,
            "foo",
            symbol_information::Kind::Function,
            &["Function docs."],
        );
        let bytes = test_index(vec![main_document(vec![], vec![module, function])]);
        let index = ScipQueryCtx::read(&bytes).unwrap();

        let symbols = index.public_symbols("main.typ");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol, function_symbol.to_owned());
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, "function");
    }

    #[test]
    fn scip_source_tokens_include_hover_and_definition() {
        let symbol = "local value";
        let bytes = test_index(vec![main_document(
            vec![
                definition_occurrence(vec![0, 0, 4], symbol),
                occurrence(vec![1, 0, 4], symbol),
            ],
            vec![symbol_info(symbol, "value", &["hello"])],
        )]);
        let index = ScipQueryCtx::read(&bytes).unwrap();

        let tokens = index.source_tokens("main.typ");
        assert_eq!(tokens.len(), 2);
        assert_eq!(
            tokens[0]
                .definition
                .as_ref()
                .unwrap()
                .target_selection_range,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            }
        );
        assert_eq!(
            tokens[1].hover.as_ref().unwrap().contents,
            HoverContents::Scalar(MarkedString::String("hello".to_owned()))
        );
        assert_eq!(
            tokens[1]
                .definition
                .as_ref()
                .unwrap()
                .target_selection_range,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            }
        );
    }

    fn test_index(documents: Vec<ScipDocument>) -> Vec<u8> {
        let path = fixture_path();
        let root = path.parent().unwrap();
        ScipIndex {
            metadata: MessageField::some(ScipMetadata {
                tool_info: MessageField::some(ScipToolInfo {
                    name: "tinymist".to_owned(),
                    version: "test".to_owned(),
                    ..Default::default()
                }),
                project_root: path_to_url(root).unwrap().to_string(),
                text_document_encoding: EnumOrUnknown::new(ScipTextEncoding::UTF8),
                ..Default::default()
            }),
            documents,
            ..Default::default()
        }
        .write_to_bytes()
        .unwrap()
    }

    fn main_document(
        occurrences: Vec<ScipOccurrence>,
        symbols: Vec<ScipSymbolInformation>,
    ) -> ScipDocument {
        ScipDocument {
            language: "typst".to_owned(),
            relative_path: "main.typ".to_owned(),
            occurrences,
            symbols,
            position_encoding: EnumOrUnknown::new(
                ScipPositionEncoding::UTF16CodeUnitOffsetFromLineStart,
            ),
            ..Default::default()
        }
    }

    fn occurrence(range: Vec<i32>, symbol: &str) -> ScipOccurrence {
        ScipOccurrence {
            range,
            symbol: symbol.to_owned(),
            ..Default::default()
        }
    }

    fn definition_occurrence(range: Vec<i32>, symbol: &str) -> ScipOccurrence {
        ScipOccurrence {
            symbol_roles: ScipSymbolRole::Definition.value(),
            ..occurrence(range, symbol)
        }
    }

    fn symbol_info(
        symbol: &str,
        display_name: &str,
        documentation: &[&str],
    ) -> ScipSymbolInformation {
        typed_symbol_info(
            symbol,
            display_name,
            symbol_information::Kind::UnspecifiedKind,
            documentation,
        )
    }

    fn typed_symbol_info(
        symbol: &str,
        display_name: &str,
        kind: symbol_information::Kind,
        documentation: &[&str],
    ) -> ScipSymbolInformation {
        ScipSymbolInformation {
            symbol: symbol.to_owned(),
            documentation: documentation.iter().map(|doc| (*doc).to_owned()).collect(),
            kind: EnumOrUnknown::new(kind),
            display_name: display_name.to_owned(),
            ..Default::default()
        }
    }

    fn public_relationship(symbol: &str) -> ScipRelationship {
        ScipRelationship {
            symbol: symbol.to_owned(),
            is_reference: true,
            is_definition: true,
            ..Default::default()
        }
    }

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("main.typ")
    }
}
