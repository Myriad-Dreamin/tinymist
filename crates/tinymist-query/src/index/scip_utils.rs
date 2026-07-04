//! Shared helpers for SCIP protobuf data.

use scip::types::{
    Relationship as ScipRelationship, SymbolInformation as ScipSymbolInformation,
    symbol_information,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScipParamGroup {
    Positional,
    Rest,
    Named,
}

impl ScipParamGroup {
    pub(super) fn descriptor(self) -> &'static str {
        match self {
            Self::Positional => "positional",
            Self::Rest => "rest",
            Self::Named => "named",
        }
    }

    pub(super) fn from_descriptor(descriptor: &str) -> Option<Self> {
        match descriptor {
            "positional" => Some(Self::Positional),
            "rest" => Some(Self::Rest),
            "named" => Some(Self::Named),
            _ => None,
        }
    }
}

pub(super) fn merge_symbol_information(
    current: &mut ScipSymbolInformation,
    incoming: ScipSymbolInformation,
) {
    if current.documentation.is_empty() {
        current.documentation = incoming.documentation;
    }
    if current.signature_documentation.is_none() {
        current.signature_documentation = incoming.signature_documentation;
    }
    if current.display_name.is_empty() {
        current.display_name = incoming.display_name;
    }
    if current.kind.enum_value().ok() == Some(symbol_information::Kind::UnspecifiedKind) {
        current.kind = incoming.kind;
    }
    if current.enclosing_symbol.is_empty() {
        current.enclosing_symbol = incoming.enclosing_symbol;
    }
    for relationship in incoming.relationships {
        push_relationship_unique(&mut current.relationships, relationship);
    }
}

pub(super) fn push_relationship_unique(
    relationships: &mut Vec<ScipRelationship>,
    relationship: ScipRelationship,
) {
    if relationships.iter().any(|current| {
        current.symbol == relationship.symbol
            && current.is_reference == relationship.is_reference
            && current.is_implementation == relationship.is_implementation
            && current.is_type_definition == relationship.is_type_definition
            && current.is_definition == relationship.is_definition
    }) {
        return;
    }

    relationships.push(relationship);
}
