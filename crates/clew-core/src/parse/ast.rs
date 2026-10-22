//! Syntax tree for the CT-MOD-002 subset of SysML v2.
//!
//! Every node keeps the 1-based line and column of its first token so later stages
//! (name resolution, the index, diagnostics) can point at the exact place in the text.

// @implements CT-MOD-002, CT-USE-007
/// 1-based line and column of a node's first character. Columns count characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: u32,
    pub column: u32,
}

// @implements CT-MOD-002
/// A name with optional `::` qualification, such as `ScalarValues::String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualifiedName {
    pub pos: Pos,
    /// Each segment without quotes, so `'a b'::c` is `["a b", "c"]`.
    pub segments: Vec<String>,
}

// @implements CT-MOD-002
impl QualifiedName {
    /// The segments joined with `::`, without quotes.
    pub fn joined(&self) -> String {
        self.segments.join("::")
    }
}

// @implements CT-MOD-002
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

// @implements CT-MOD-002
/// One member of a package or of a definition or usage body.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Package(Package),
    Import(Import),
    Comment(Comment),
    Definition(Definition),
    Usage(Usage),
    Satisfy(Satisfy),
    Verify(Verify),
    Dependency(Dependency),
}

// @implements CT-MOD-002
impl Member {
    /// Position of the member's first token.
    pub fn pos(&self) -> Pos {
        match self {
            Member::Package(m) => m.pos,
            Member::Import(m) => m.pos,
            Member::Comment(m) => m.pos,
            Member::Definition(m) => m.pos,
            Member::Usage(m) => m.pos,
            Member::Satisfy(m) => m.pos,
            Member::Verify(m) => m.pos,
            Member::Dependency(m) => m.pos,
        }
    }

    /// The members nested inside this one, empty for imports and comments.
    pub fn members(&self) -> &[Member] {
        match self {
            Member::Package(m) => &m.members,
            Member::Definition(m) => &m.members,
            Member::Usage(m) => &m.members,
            Member::Satisfy(m) => &m.members,
            Member::Verify(m) => &m.members,
            Member::Dependency(m) => &m.members,
            Member::Import(_) | Member::Comment(_) => &[],
        }
    }
}

// @implements CT-MOD-002
/// `package <'short'> Name { ... }` or `package Name;`.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub pos: Pos,
    pub visibility: Option<Visibility>,
    pub short_name: Option<String>,
    pub name: Option<String>,
    pub members: Vec<Member>,
}

// @implements CT-MOD-002
/// What an import brings in after its qualified name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `import A::B;` imports the one element.
    Element,
    /// `import A::*;` imports the members of `A`.
    Members,
    /// `import A::**;` imports `A` and everything nested in it.
    Recursive,
    /// `import A::*::**;` imports everything nested in `A`.
    MembersRecursive,
}

// @implements CT-MOD-002
/// `private import all A::*;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub pos: Pos,
    pub visibility: Option<Visibility>,
    /// True for `import all`, which also imports private members.
    pub all: bool,
    pub target: QualifiedName,
    pub kind: ImportKind,
}

// @implements CT-MOD-002
/// `doc /* ... */`, `comment name about x /* ... */`, or a bare `/* ... */`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub pos: Pos,
    /// True for `doc`, false for `comment` and bare comments.
    pub is_doc: bool,
    pub short_name: Option<String>,
    pub name: Option<String>,
    pub about: Vec<QualifiedName>,
    /// The body with `/*`, `*/`, and each line's leading `*` removed. Lines keep their
    /// order and are joined by `\n`, with surrounding blank lines dropped.
    pub body: String,
}

// @implements CT-MOD-002
/// The element kinds that have both a definition and a usage form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Part,
    Attribute,
    Port,
    Requirement,
}

// @implements CT-MOD-002
/// `abstract part def <'P1'> Vehicle :> Thing { ... }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub pos: Pos,
    pub visibility: Option<Visibility>,
    pub kind: Kind,
    pub is_abstract: bool,
    pub short_name: Option<String>,
    pub name: Option<String>,
    /// Targets of `:>` or `specializes`.
    pub specializes: Vec<QualifiedName>,
    pub members: Vec<Member>,
}

// @implements CT-MOD-002
/// Direction of a feature, written before its keyword: `in attribute x;`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    InOut,
}

// @implements CT-MOD-002
/// What a usage is: one of the definable kinds, or a requirement's `subject`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageKind {
    Of(Kind),
    Subject,
}

// @implements CT-MOD-002
/// `part <'W1'> wheel [4] : Wheel :> parts :>> base = value { ... }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub pos: Pos,
    pub visibility: Option<Visibility>,
    pub kind: UsageKind,
    pub direction: Option<Direction>,
    pub is_abstract: bool,
    pub short_name: Option<String>,
    pub name: Option<String>,
    /// The text between `[` and `]`, without spaces, such as `0..*`.
    pub multiplicity: Option<String>,
    /// Targets of `:` or `defined by`.
    pub types: Vec<QualifiedName>,
    /// Targets of `:>` or `subsets`.
    pub subsets: Vec<QualifiedName>,
    /// Targets of `:>>` or `redefines`.
    pub redefines: Vec<QualifiedName>,
    pub value: Option<Value>,
    pub members: Vec<Member>,
}

// @implements CT-MOD-002
impl Usage {
    /// The declared name, or for `:>> x` with no name of its own, the last segment of the
    /// first redefined feature, which is the name the usage takes in SysML v2.
    pub fn effective_name(&self) -> Option<&str> {
        self.name.as_deref().or_else(|| {
            self.redefines
                .first()
                .and_then(|r| r.segments.last())
                .map(String::as_str)
        })
    }
}

// @implements CT-MOD-002
/// The kind of a feature value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    String,
    Integer,
    Real,
    Boolean,
    /// A reference such as `Colors::red`.
    Name,
    /// Any longer expression, kept as source text for later tasks.
    Expression,
}

// @implements CT-MOD-002
/// A feature value after `=`, `:=`, or `default`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub pos: Pos,
    pub kind: ValueKind,
    /// The value's meaning: a string without quotes and with escapes resolved, a name
    /// without quotes, or the source text of a number, boolean, or expression.
    pub text: String,
    /// True when written with `default`.
    pub is_default: bool,
}

// @implements CT-MOD-002
/// `satisfy requirement R by part;`.
#[derive(Debug, Clone, PartialEq)]
pub struct Satisfy {
    pub pos: Pos,
    pub requirement: QualifiedName,
    pub by: Option<QualifiedName>,
    pub members: Vec<Member>,
}

// @implements CT-MOD-002
/// `verify requirement R;` inside a verification objective, or at any member position.
#[derive(Debug, Clone, PartialEq)]
pub struct Verify {
    pub pos: Pos,
    pub requirement: QualifiedName,
    pub members: Vec<Member>,
}

// @implements CT-TRC-002
/// The trace meaning of a dependency, given by the metadata written before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    /// `#derivation`: each source is derived from each target.
    Derivation,
    /// `#refinement`: each source refines each target.
    Refinement,
    /// `#evidence`: each source is evidence for each target.
    Evidence,
}

// @implements CT-TRC-002
/// `#refinement dependency <'D1'> name from a, b to c;` or `#derivation dependency a to b;`.
#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub pos: Pos,
    pub visibility: Option<Visibility>,
    pub kind: DependencyKind,
    pub short_name: Option<String>,
    pub name: Option<String>,
    /// The names before `to`, which SysML v2 calls the clients.
    pub clients: Vec<QualifiedName>,
    /// The names after `to`, which SysML v2 calls the suppliers.
    pub suppliers: Vec<QualifiedName>,
    pub members: Vec<Member>,
}
