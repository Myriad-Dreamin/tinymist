//! SCIP index encoding.

use core::fmt::Write as _;
use std::sync::Arc;

use crate::analysis::SharedContext;
use crate::prelude::Definition;
use crate::syntax::DefKind;
use lsp_types::{Hover, HoverContents, MarkedString, Range};
use protobuf::{Enum, EnumOrUnknown, Message, MessageField};
use scip::types::{
    Descriptor as ScipDescriptor, Document as ScipDocument, Index as ScipIndex,
    Metadata as ScipMetadata, Occurrence as ScipOccurrence, Package as ScipPackage,
    PositionEncoding as ScipPositionEncoding, Relationship as ScipRelationship, Signature,
    Symbol as ScipSymbol, SymbolInformation as ScipSymbolInformation, SymbolRole as ScipSymbolRole,
    TextEncoding as ScipTextEncoding, ToolInfo as ScipToolInfo, descriptor, symbol_information,
};
use tinymist_analysis::docs::{DefDocs, ParamDocs, SignatureDocs};
use tinymist_std::error::WithContextUntyped;
use tinymist_std::hash::FxHashMap;
use typst::syntax::{FileId, Source, Span};
use typst_shim::syntax::source_range;

use super::scip_utils::{ScipParamGroup, merge_symbol_information, push_relationship_unique};
use super::{FileIndex, KnowledgeWithContext};

/// Public package API information encoded as SCIP relationships.
#[derive(Debug, Clone, Default)]
pub struct ScipPublicApi {
    /// Public modules in the package docs tree.
    pub modules: Vec<ScipPublicModule>,
}

/// Public symbols exported by one module.
#[derive(Debug, Clone, Default)]
pub struct ScipPublicModule {
    /// The module file path, relative to the package root.
    pub file_path: Option<String>,
    /// The SCIP symbol of the module.
    pub module_symbol: Option<String>,
    /// Public SCIP symbols exported by the module.
    pub public_symbols: Vec<String>,
}

impl KnowledgeWithContext<'_> {
    /// Dumps the knowledge in SCIP protobuf format.
    pub fn to_scip_bytes(&self) -> tinymist_std::Result<Vec<u8>> {
        let index = self.to_scip_index()?;
        index
            .write_to_bytes()
            .context_ut("cannot serialize SCIP index")
    }

    /// Dumps the knowledge in SCIP protobuf format with public package API data.
    pub fn to_scip_bytes_with_public_api(
        &self,
        public_api: &ScipPublicApi,
    ) -> tinymist_std::Result<Vec<u8>> {
        let index = self.to_scip_index_with_public_api(public_api)?;
        index
            .write_to_bytes()
            .context_ut("cannot serialize SCIP index")
    }

    /// Dumps the knowledge as a SCIP index.
    pub fn to_scip_index(&self) -> tinymist_std::Result<ScipIndex> {
        self.to_scip_index_with_public_api(&ScipPublicApi::default())
    }

    /// Dumps the knowledge as a SCIP index with public package API data.
    pub fn to_scip_index_with_public_api(
        &self,
        public_api: &ScipPublicApi,
    ) -> tinymist_std::Result<ScipIndex> {
        let mut encoder = ScipEncoder::new(self.ctx);
        encoder.emit_files(&self.knowledge.files)?;
        encoder.emit_public_api(public_api);
        let (documents, external_symbols) = encoder.finish();
        Ok(ScipIndex {
            metadata: MessageField::some(ScipMetadata {
                tool_info: MessageField::some(ScipToolInfo {
                    name: "tinymist".to_owned(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    ..Default::default()
                }),
                project_root: self.knowledge.meta.project_root.to_string(),
                text_document_encoding: EnumOrUnknown::new(ScipTextEncoding::UTF8),
                ..Default::default()
            }),
            documents,
            external_symbols,
            ..Default::default()
        })
    }
}

struct ScipEncoder<'a> {
    ctx: &'a Arc<SharedContext>,
    documents: Vec<ScipDocumentBuilder>,
    document_by_fid: FxHashMap<FileId, usize>,
    symbols: FxHashMap<Definition, String>,
    external_symbols: FxHashMap<String, ScipSymbolInformation>,
}

struct ScipDocumentBuilder {
    relative_path: String,
    occurrences: FxHashMap<ScipOccurrenceKey, ScipOccurrence>,
    symbols: FxHashMap<String, ScipSymbolInformation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ScipOccurrenceKey {
    range: Vec<i32>,
    symbol: String,
}

impl<'a> ScipEncoder<'a> {
    fn new(ctx: &'a Arc<SharedContext>) -> Self {
        Self {
            ctx,
            documents: Vec::new(),
            document_by_fid: FxHashMap::default(),
            symbols: FxHashMap::default(),
            external_symbols: FxHashMap::default(),
        }
    }

    fn emit_files(&mut self, files: &[FileIndex]) -> tinymist_std::Result<()> {
        for file in files {
            let _ = self.document_index(file.fid)?;
        }

        for (idx, file) in files.iter().enumerate() {
            eprintln!("emit scip file: {:?}, {idx} of {}", file.fid, files.len());
            let source = self
                .ctx
                .source_by_id(file.fid)
                .context_ut("cannot get source")?;
            let document_index = self.document_index(file.fid)?;

            for (span, reference) in &file.references {
                let symbol = self.symbol(&reference.definition)?;
                let docs = reference.hover.as_ref().and_then(hover_to_markdown);
                let def_docs = reference.def_docs.as_ref();

                if let Some(range) = lsp_range(self.ctx, *span, &source) {
                    self.documents[document_index].push_occurrence(ScipOccurrence {
                        range: scip_range(range),
                        symbol: symbol.clone(),
                        ..Default::default()
                    });
                }

                if let Some((def_fid, def_range)) = self.definition_range(&reference.definition) {
                    let def_document_index = self.document_index(def_fid)?;
                    self.documents[def_document_index].push_occurrence(ScipOccurrence {
                        range: scip_range(def_range),
                        symbol: symbol.clone(),
                        symbol_roles: ScipSymbolRole::Definition.value(),
                        ..Default::default()
                    });
                    for info in symbol_infos(&reference.definition, &symbol, def_docs, docs) {
                        self.documents[def_document_index].push_symbol(info);
                    }
                } else if let Some(info) =
                    symbol_info_external(&reference.definition, &symbol, def_docs, docs)
                {
                    for item in info {
                        self.external_symbols
                            .entry(item.symbol.clone())
                            .or_insert(item);
                    }
                }
            }
        }

        Ok(())
    }

    fn document_index(&mut self, fid: FileId) -> tinymist_std::Result<usize> {
        if let Some(index) = self.document_by_fid.get(&fid) {
            return Ok(*index);
        }

        let _ = self
            .ctx
            .source_by_id(fid)
            .context_ut("cannot get source for SCIP document")?;
        let index = self.documents.len();
        self.documents.push(ScipDocumentBuilder {
            relative_path: fid.vpath().get_without_slash().to_owned(),
            occurrences: FxHashMap::default(),
            symbols: FxHashMap::default(),
        });
        self.document_by_fid.insert(fid, index);
        Ok(index)
    }

    fn symbol(&mut self, definition: &Definition) -> tinymist_std::Result<String> {
        if let Some(symbol) = self.symbols.get(definition) {
            return Ok(symbol.clone());
        }

        let symbol = scip_symbol(self.ctx, definition)?;
        self.symbols.insert(definition.clone(), symbol.clone());
        Ok(symbol)
    }

    fn definition_range(&self, definition: &Definition) -> Option<(FileId, Range)> {
        let fid = definition.file_id()?;
        let source = self.ctx.source_by_id(fid).ok()?;
        let span = definition.decl.span();
        if span.is_detached() {
            return None;
        }

        Some((fid, lsp_range(self.ctx, span, &source)?))
    }

    fn emit_public_api(&mut self, public_api: &ScipPublicApi) {
        for module in &public_api.modules {
            let Some(module_symbol) = module.module_symbol.as_ref() else {
                continue;
            };

            if let Some(document_index) = module
                .file_path
                .as_deref()
                .and_then(|path| self.document_index_by_path(path))
            {
                self.documents[document_index].push_symbol(public_module_symbol_info(
                    module_symbol,
                    &module.public_symbols,
                ));
            }

            for public_symbol in &module.public_symbols {
                self.push_symbol_relationship(
                    public_symbol,
                    ScipRelationship {
                        symbol: module_symbol.clone(),
                        is_reference: true,
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn document_index_by_path(&self, path: &str) -> Option<usize> {
        self.documents
            .iter()
            .position(|document| document.relative_path == path)
    }

    fn push_symbol_relationship(&mut self, symbol: &str, relationship: ScipRelationship) {
        for document in &mut self.documents {
            if document.push_relationship(symbol, relationship.clone()) {
                return;
            }
        }

        if let Some(info) = self.external_symbols.get_mut(symbol) {
            push_relationship_unique(&mut info.relationships, relationship);
            return;
        }

        self.external_symbols.insert(
            symbol.to_owned(),
            ScipSymbolInformation {
                symbol: symbol.to_owned(),
                relationships: vec![relationship],
                ..Default::default()
            },
        );
    }

    fn finish(self) -> (Vec<ScipDocument>, Vec<ScipSymbolInformation>) {
        let documents = self
            .documents
            .into_iter()
            .map(ScipDocumentBuilder::finish)
            .collect();
        let mut external_symbols = self.external_symbols.into_values().collect::<Vec<_>>();
        external_symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        (documents, external_symbols)
    }
}

impl ScipDocumentBuilder {
    fn push_occurrence(&mut self, occurrence: ScipOccurrence) {
        let key = ScipOccurrenceKey {
            range: occurrence.range.clone(),
            symbol: occurrence.symbol.clone(),
        };
        if let Some(current) = self.occurrences.get_mut(&key) {
            current.symbol_roles |= occurrence.symbol_roles;
            if current.override_documentation.is_empty() {
                current.override_documentation = occurrence.override_documentation;
            }
            return;
        }

        self.occurrences.insert(key, occurrence);
    }

    fn push_symbol(&mut self, symbol: ScipSymbolInformation) {
        if let Some(current) = self.symbols.get_mut(&symbol.symbol) {
            merge_symbol_information(current, symbol);
            return;
        }

        self.symbols.insert(symbol.symbol.clone(), symbol);
    }

    fn push_relationship(&mut self, symbol: &str, relationship: ScipRelationship) -> bool {
        let Some(current) = self.symbols.get_mut(symbol) else {
            return false;
        };
        push_relationship_unique(&mut current.relationships, relationship);
        true
    }

    fn finish(self) -> ScipDocument {
        let mut occurrences = self.occurrences.into_values().collect::<Vec<_>>();
        occurrences.sort_by(|left, right| {
            left.range
                .cmp(&right.range)
                .then_with(|| left.symbol.cmp(&right.symbol))
        });
        let mut symbols = self.symbols.into_values().collect::<Vec<_>>();
        symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        ScipDocument {
            language: "typst".to_owned(),
            relative_path: self.relative_path,
            occurrences,
            symbols,
            position_encoding: EnumOrUnknown::new(
                ScipPositionEncoding::UTF16CodeUnitOffsetFromLineStart,
            ),
            ..Default::default()
        }
    }
}

fn lsp_range(ctx: &SharedContext, span: Span, source: &Source) -> Option<Range> {
    let range = source_range(source, span)?;
    Some(ctx.to_lsp_range(range, source))
}

fn scip_range(range: Range) -> Vec<i32> {
    if range.start.line == range.end.line {
        vec![
            range.start.line as i32,
            range.start.character as i32,
            range.end.character as i32,
        ]
    } else {
        vec![
            range.start.line as i32,
            range.start.character as i32,
            range.end.line as i32,
            range.end.character as i32,
        ]
    }
}

pub(crate) fn scip_symbol(
    ctx: &SharedContext,
    definition: &Definition,
) -> tinymist_std::Result<String> {
    let disambiguator = definition_disambiguator(ctx, definition);
    scip_symbol_with_disambiguator(definition, disambiguator)
}

pub(crate) fn scip_symbol_with_disambiguator(
    definition: &Definition,
    disambiguator: String,
) -> tinymist_std::Result<String> {
    let mut descriptors = Vec::new();
    if let Some(fid) = definition.file_id() {
        let path = fid.vpath().get_without_slash();
        for part in path.split('/') {
            if !part.is_empty() {
                descriptors.push(scip_descriptor(part, descriptor::Suffix::Namespace, ""));
            }
        }
    } else {
        descriptors.push(scip_descriptor(
            "external",
            descriptor::Suffix::Namespace,
            "",
        ));
    }

    descriptors.push(scip_descriptor(
        &disambiguator,
        descriptor::Suffix::Meta,
        "",
    ));
    descriptors.push(scip_descriptor(
        definition.name().as_ref(),
        scip_symbol_suffix(definition.decl.kind()),
        "",
    ));

    let symbol = ScipSymbol {
        scheme: "typst".to_owned(),
        package: MessageField::some(ScipPackage {
            manager: "tinymist".to_owned(),
            name: "workspace".to_owned(),
            ..Default::default()
        }),
        descriptors,
        ..Default::default()
    };
    Ok(scip::symbol::format_symbol(symbol))
}

fn definition_disambiguator(ctx: &SharedContext, definition: &Definition) -> String {
    if let Some(fid) = definition.file_id()
        && let Ok(source) = ctx.source_by_id(fid)
        && let Some(range) = lsp_range(ctx, definition.decl.span(), &source)
    {
        return format!("L{}_C{}", range.start.line, range.start.character);
    }

    format!("{:x}", definition.decl.span().into_raw())
}

fn scip_descriptor(name: &str, suffix: descriptor::Suffix, disambiguator: &str) -> ScipDescriptor {
    ScipDescriptor {
        name: if name.is_empty() {
            "_".to_owned()
        } else {
            name.to_owned()
        },
        disambiguator: disambiguator.to_owned(),
        suffix: EnumOrUnknown::new(suffix),
        ..Default::default()
    }
}

fn scip_symbol_suffix(kind: DefKind) -> descriptor::Suffix {
    match kind {
        DefKind::Function => descriptor::Suffix::Method,
        DefKind::Module => descriptor::Suffix::Namespace,
        _ => descriptor::Suffix::Term,
    }
}

fn symbol_infos(
    definition: &Definition,
    symbol: &str,
    def_docs: Option<&DefDocs>,
    docs: Option<String>,
) -> Vec<ScipSymbolInformation> {
    if let Some(DefDocs::Function(docs)) = def_docs {
        return function_symbol_infos(definition, symbol, docs);
    }

    vec![ScipSymbolInformation {
        symbol: symbol.to_owned(),
        documentation: docs.into_iter().collect(),
        kind: EnumOrUnknown::new(scip_symbol_kind(definition.decl.kind())),
        display_name: definition.name().to_string(),
        ..Default::default()
    }]
}

fn symbol_info_external(
    definition: &Definition,
    symbol: &str,
    def_docs: Option<&DefDocs>,
    docs: Option<String>,
) -> Option<Vec<ScipSymbolInformation>> {
    if def_docs.is_none() {
        docs.as_ref()?;
    }

    Some(symbol_infos(definition, symbol, def_docs, docs))
}

fn scip_symbol_kind(kind: DefKind) -> symbol_information::Kind {
    match kind {
        DefKind::Function => symbol_information::Kind::Function,
        DefKind::Module => symbol_information::Kind::Module,
        DefKind::Constant => symbol_information::Kind::Constant,
        DefKind::Variable => symbol_information::Kind::Variable,
        _ => symbol_information::Kind::UnspecifiedKind,
    }
}

fn public_module_symbol_info(
    module_symbol: &str,
    public_symbols: &[String],
) -> ScipSymbolInformation {
    ScipSymbolInformation {
        symbol: module_symbol.to_owned(),
        relationships: public_symbols
            .iter()
            .map(|symbol| ScipRelationship {
                symbol: symbol.clone(),
                is_reference: true,
                is_definition: true,
                ..Default::default()
            })
            .collect(),
        kind: EnumOrUnknown::new(symbol_information::Kind::Module),
        ..Default::default()
    }
}

fn function_symbol_infos(
    definition: &Definition,
    symbol: &str,
    docs: &SignatureDocs,
) -> Vec<ScipSymbolInformation> {
    let signature_text = function_signature_text(definition.name().as_ref(), docs);
    let params = function_parameters(docs);
    let mut signature = Signature {
        language: "typc".to_owned(),
        text: signature_text.clone(),
        ..Default::default()
    };
    let mut infos = Vec::with_capacity(params.len() + 1);

    for param in params {
        let Some(param_symbol) =
            scip_parameter_symbol(symbol, param.group, param.docs.name.as_ref())
        else {
            continue;
        };
        if let Some(range) =
            signature_parameter_range(&signature_text, param.docs.name.as_ref(), param.group)
        {
            signature.occurrences.push(ScipOccurrence {
                range,
                symbol: param_symbol.clone(),
                symbol_roles: ScipSymbolRole::Definition.value(),
                ..Default::default()
            });
        }

        infos.push(parameter_symbol_info(symbol, &param_symbol, param.docs));
    }

    infos.insert(
        0,
        ScipSymbolInformation {
            symbol: symbol.to_owned(),
            documentation: non_empty_docs(docs.docs.trim()),
            kind: EnumOrUnknown::new(symbol_information::Kind::Function),
            display_name: definition.name().to_string(),
            signature_documentation: MessageField::some(signature),
            ..Default::default()
        },
    );
    infos
}

fn function_signature_text(name: &str, docs: &SignatureDocs) -> String {
    let mut text = String::new();
    text.push_str("let ");
    text.push_str(name);
    let _ = docs.print(&mut text);
    if let Some((short, _, _)) = docs.ret_ty.as_ref()
        && short != name
    {
        let _ = write!(text, " = {short}");
    }
    text.push(';');
    text
}

struct ScipParamRef<'a> {
    group: ScipParamGroup,
    docs: &'a ParamDocs,
}

fn function_parameters(docs: &SignatureDocs) -> Vec<ScipParamRef<'_>> {
    let mut params =
        Vec::with_capacity(docs.pos.len() + docs.named.len() + usize::from(docs.rest.is_some()));
    params.extend(docs.pos.iter().map(|docs| ScipParamRef {
        group: ScipParamGroup::Positional,
        docs,
    }));
    if let Some(docs) = docs.rest.as_ref() {
        params.push(ScipParamRef {
            group: ScipParamGroup::Rest,
            docs,
        });
    }
    params.extend(docs.named.values().map(|docs| ScipParamRef {
        group: ScipParamGroup::Named,
        docs,
    }));
    params
}

fn scip_parameter_symbol(
    function_symbol: &str,
    group: ScipParamGroup,
    name: &str,
) -> Option<String> {
    let mut symbol = scip::symbol::parse_symbol(function_symbol).ok()?;
    symbol.descriptors.push(scip_descriptor(
        group.descriptor(),
        descriptor::Suffix::Meta,
        "",
    ));
    symbol
        .descriptors
        .push(scip_descriptor(name, descriptor::Suffix::Parameter, ""));
    Some(scip::symbol::format_symbol(symbol))
}

fn signature_parameter_range(
    signature: &str,
    name: &str,
    group: ScipParamGroup,
) -> Option<Vec<i32>> {
    let mut line_offset = 0usize;
    for line in signature.lines() {
        let trimmed = line.trim_start();
        let candidate = match group {
            ScipParamGroup::Rest => {
                let Some(candidate) = trimmed.strip_prefix("..") else {
                    line_offset += 1;
                    continue;
                };
                candidate
            }
            _ => trimmed,
        };

        let matches = candidate.strip_prefix(name).is_some_and(|rest| {
            matches!(
                rest.chars().next(),
                Some(':') | Some(',') | Some(' ') | Some('=') | None
            )
        });
        if matches {
            let leading_chars = line.chars().count() - trimmed.chars().count();
            let prefix_chars = if group == ScipParamGroup::Rest { 2 } else { 0 };
            let start = leading_chars + prefix_chars;
            let end = start + name.chars().count();
            return Some(vec![line_offset as i32, start as i32, end as i32]);
        }

        line_offset += 1;
    }

    None
}

fn parameter_symbol_info(
    function_symbol: &str,
    symbol: &str,
    docs: &ParamDocs,
) -> ScipSymbolInformation {
    ScipSymbolInformation {
        symbol: symbol.to_owned(),
        documentation: parameter_documentation(docs),
        kind: EnumOrUnknown::new(symbol_information::Kind::Parameter),
        display_name: docs.name.to_string(),
        enclosing_symbol: function_symbol.to_owned(),
        ..Default::default()
    }
}

fn parameter_documentation(docs: &ParamDocs) -> Vec<String> {
    let mut output = Vec::new();
    if let Some((_, _, ty)) = docs.cano_type.as_ref()
        && !ty.trim().is_empty()
    {
        output.push(format!("```typc\ntype: {}\n```", ty.trim()));
    }
    output.extend(non_empty_docs(docs.docs.trim()));
    output
}

fn non_empty_docs(docs: &str) -> Vec<String> {
    if docs.trim().is_empty() {
        Vec::new()
    } else {
        vec![docs.trim().to_owned()]
    }
}

fn hover_to_markdown(hover: &Hover) -> Option<String> {
    hover_contents_to_markdown(&hover.contents)
}

fn hover_contents_to_markdown(contents: &HoverContents) -> Option<String> {
    match contents {
        HoverContents::Scalar(marked) => marked_string_to_markdown(marked),
        HoverContents::Array(parts) => {
            let parts = parts
                .iter()
                .filter_map(marked_string_to_markdown)
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n\n---\n\n"))
            }
        }
        HoverContents::Markup(markup) => {
            if markup.value.trim().is_empty() {
                None
            } else {
                Some(markup.value.clone())
            }
        }
    }
}

fn marked_string_to_markdown(marked: &MarkedString) -> Option<String> {
    match marked {
        MarkedString::String(value) => {
            if value.trim().is_empty() {
                None
            } else {
                Some(value.clone())
            }
        }
        MarkedString::LanguageString(value) => {
            if value.value.trim().is_empty() {
                None
            } else {
                Some(format!("```{}\n{}\n```", value.language, value.value))
            }
        }
    }
}
