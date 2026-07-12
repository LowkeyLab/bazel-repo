#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemplateAst(pub(super) Vec<TemplateNode>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TemplateNode {
    Text(String),
    Field(String),
    FrontSide,
    ClozeField(String),
    Section {
        name: String,
        kind: SectionKind,
        children: Vec<TemplateNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SectionKind {
    Positive,
    Inverted,
}
