//! Model and name resolution for the CT-MOD-002 subset of SysML v2.
//!
//! `Model::build` gathers parsed files into one tree of elements under a shared root
//! namespace, then resolves every name the files cite: import targets, specializations,
//! types, subsetted and redefined features, satisfy and verify targets, and the elements a
//! comment is `about`. Each name that does not resolve becomes an error with file, line,
//! column, and help.
//!
//! Lookup follows SysML v2 in simplified form. An unqualified name is searched in the
//! namespace that cites it, then in each enclosing namespace out to the root. Within a
//! namespace the search covers owned members, members inherited from what the namespace
//! specializes, is typed by, subsets, or redefines, and imported members. Later segments of
//! a qualified name see only public members. A built-in `ScalarValues` package stands in for
//! the standard library.

use std::collections::{HashMap, VecDeque};
use std::fmt;

use crate::parse::ast::{ImportKind, Kind, Member, Pos, QualifiedName, UsageKind, Visibility};
use crate::parse::parser::{self, SyntaxError};

// @implements CT-MOD-002
/// The part of the SysML v2 standard library that is built in: the scalar value types.
const LIBRARY: &str = "package ScalarValues {
    abstract attribute def ScalarValue;
    attribute def Boolean :> ScalarValue;
    attribute def String :> ScalarValue;
    abstract attribute def NumericalValue :> ScalarValue;
    abstract attribute def Number :> NumericalValue;
    attribute def Complex :> Number;
    attribute def Real :> Complex;
    attribute def Rational :> Real;
    attribute def Integer :> Rational;
    attribute def Natural :> Integer;
    attribute def Positive :> Natural;
}
";

// @implements CT-MOD-002
/// The file name the built-in library is listed under in `Model::files`.
pub const LIBRARY_PATH: &str = "<built-in library>";

// @implements CT-MOD-002
/// Index of an element in `Model::elements`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(pub usize);

// @implements CT-MOD-002
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    /// The root namespace shared by every file.
    Root,
    Package,
    Definition(Kind),
    Usage(UsageKind),
    Satisfy,
    Verify,
}

// @implements CT-MOD-002
/// A namespace member that can be named, with the namespaces it inherits from and imports.
#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementKind,
    /// The declared name, or for `:>> x` with no name of its own, `x`.
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub visibility: Visibility,
    pub owner: Option<ElementId>,
    /// Index into `Model::files`.
    pub file: usize,
    pub pos: Pos,
    pub members: Vec<ElementId>,
    /// Owned members by name and by short name, in declaration order.
    by_name: HashMap<String, Vec<ElementId>>,
    /// Indexes into `Model::references` of the elements this one inherits members from.
    generals: Vec<usize>,
    imports: Vec<ImportDecl>,
}

// @implements CT-MOD-002
#[derive(Debug, Clone)]
struct ImportDecl {
    /// Index into `Model::references`.
    reference: usize,
    kind: ImportKind,
    all: bool,
    public: bool,
}

// @implements CT-MOD-002
/// What a cited name is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Import,
    Specializes,
    Type,
    Subsets,
    Redefines,
    /// The requirement of a `satisfy` or `verify`.
    Requirement,
    /// The element after `satisfy ... by`.
    SatisfiedBy,
    /// An element a comment is `about`.
    About,
}

// @implements CT-MOD-002, CT-USE-007
impl Role {
    fn noun(self) -> &'static str {
        match self {
            Role::Import => "import target",
            Role::Specializes => "specialized definition",
            Role::Type => "type",
            Role::Subsets => "subsetted feature",
            Role::Redefines => "redefined feature",
            Role::Requirement => "requirement",
            Role::SatisfiedBy => "satisfying element",
            Role::About => "annotated element",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Role::Import => {
                "check the spelling and that the file declaring it is in the project; \
                 only ScalarValues of the standard library is built in"
            }
            Role::Subsets | Role::Redefines => {
                "the feature must be declared in a type or definition this element is \
                 typed by or specializes, or be visible from here"
            }
            _ => {
                "check the spelling, or import the package that declares it, for example \
                  `private import ScalarValues::*;`"
            }
        }
    }
}

// @implements CT-MOD-002
/// A name cited in the model and the element it resolves to.
#[derive(Debug, Clone)]
pub struct Reference {
    pub role: Role,
    /// Index into `Model::files`.
    pub file: usize,
    pub name: QualifiedName,
    /// The element whose declaration cites the name, or for imports and comments, the
    /// namespace that holds them.
    pub source: ElementId,
    pub target: Option<ElementId>,
    /// The namespace where lookup starts.
    scope: ElementId,
    /// The citing element itself, which never resolves its own names.
    skip: Option<ElementId>,
}

// @implements CT-MOD-002, CT-USE-007
/// An unresolved name: the file it is in and where, what, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    pub path: String,
    pub error: SyntaxError,
}

// @implements CT-USE-007
impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error.render(&self.path))
    }
}

// @implements CT-MOD-002, CT-USE-007
/// Every element and cited name of a set of files, with names resolved.
#[derive(Debug, Clone)]
pub struct Model {
    /// File paths, the built-in library first.
    pub files: Vec<String>,
    /// Elements, the root first.
    pub elements: Vec<Element>,
    pub references: Vec<Reference>,
    /// Unresolved names, sorted by file, line, and column.
    pub errors: Vec<ModelError>,
}

// @implements CT-MOD-002, CT-USE-007
impl Model {
    /// The root namespace that holds every file's top-level members.
    pub const ROOT: ElementId = ElementId(0);

    /// Builds the model of parsed files, given as `(path, members)`, and resolves its names.
    pub fn build(files: &[(&str, &[Member])]) -> Model {
        let library = parser::parse(LIBRARY);
        let mut model = Model {
            files: Vec::new(),
            elements: vec![Element::new(ElementKind::Root, None, 0, Pos::default())],
            references: Vec::new(),
            errors: Vec::new(),
        };
        model.add_file(LIBRARY_PATH, &library.members);
        for (path, members) in files {
            model.add_file(path, members);
        }
        model.resolve();
        model
    }

    pub fn element(&self, id: ElementId) -> &Element {
        &self.elements[id.0]
    }

    /// Resolves a qualified name such as `P::Q::'a b'` from the root.
    pub fn find(&self, qualified: &str) -> Option<ElementId> {
        let segments: Vec<String> = qualified
            .split("::")
            .map(|s| s.trim_matches('\'').to_string())
            .collect();
        Resolver::new(self)
            .resolve(Model::ROOT, &segments, None)
            .ok()
    }

    /// The element's names from the root down, joined by `::`. Names that are not plain
    /// identifiers are quoted, and an element with no name uses its short name.
    pub fn qualified_name(&self, id: ElementId) -> String {
        let mut parts = Vec::new();
        let mut at = Some(id);
        while let Some(current) = at.filter(|&a| a != Model::ROOT) {
            let element = self.element(current);
            let name = element.name.as_deref().or(element.short_name.as_deref());
            parts.push(name.map_or_else(|| "(unnamed)".to_string(), quote));
            at = element.owner;
        }
        parts.reverse();
        parts.join("::")
    }
}

// @implements CT-MOD-002
/// Building the element tree and recording the names each element cites.
impl Model {
    fn add_file(&mut self, path: &str, members: &[Member]) {
        let file = self.files.len();
        self.files.push(path.to_string());
        self.add_members(Model::ROOT, file, members);
    }

    fn add_element(&mut self, owner: ElementId, mut element: Element) -> ElementId {
        let id = ElementId(self.elements.len());
        element.owner = Some(owner);
        let parent = &mut self.elements[owner.0];
        parent.members.push(id);
        for key in [&element.name, &element.short_name].into_iter().flatten() {
            parent.by_name.entry(key.clone()).or_default().push(id);
        }
        self.elements.push(element);
        id
    }

    /// Records a cited name. Names an element cites about itself, such as its types, are
    /// looked up from its owner and become what it inherits from when `general` is set.
    fn cite(&mut self, role: Role, file: usize, name: &QualifiedName, from: Cite) -> usize {
        let index = self.references.len();
        let (source, scope, skip) = match from {
            Cite::Itself(id) => (id, self.element(id).owner.unwrap_or(Model::ROOT), Some(id)),
            Cite::Within(ns) => (ns, ns, None),
        };
        self.references.push(Reference {
            role,
            file,
            name: name.clone(),
            source,
            target: None,
            scope,
            skip,
        });
        if let Cite::Itself(id) = from
            && !matches!(role, Role::SatisfiedBy)
        {
            self.elements[id.0].generals.push(index);
        }
        index
    }

    fn add_members(&mut self, ns: ElementId, file: usize, members: &[Member]) {
        for member in members {
            let (element, cited, nested): (Element, Vec<(Role, &QualifiedName)>, &[Member]) =
                match member {
                    Member::Import(i) => {
                        let reference = self.cite(Role::Import, file, &i.target, Cite::Within(ns));
                        self.elements[ns.0].imports.push(ImportDecl {
                            reference,
                            kind: i.kind,
                            all: i.all,
                            public: i.visibility.unwrap_or(Visibility::Public)
                                == Visibility::Public,
                        });
                        continue;
                    }
                    Member::Comment(c) => {
                        for about in &c.about {
                            self.cite(Role::About, file, about, Cite::Within(ns));
                        }
                        continue;
                    }
                    Member::Package(p) => {
                        let mut e = Element::new(ElementKind::Package, p.name.clone(), file, p.pos);
                        e.short_name.clone_from(&p.short_name);
                        e.visibility = p.visibility.unwrap_or(Visibility::Public);
                        (e, Vec::new(), &p.members)
                    }
                    Member::Definition(d) => {
                        let kind = ElementKind::Definition(d.kind);
                        let mut e = Element::new(kind, d.name.clone(), file, d.pos);
                        e.short_name.clone_from(&d.short_name);
                        e.visibility = d.visibility.unwrap_or(Visibility::Public);
                        let cited = d.specializes.iter().map(|n| (Role::Specializes, n));
                        (e, cited.collect(), &d.members)
                    }
                    Member::Usage(u) => {
                        let name = u.effective_name().map(str::to_string);
                        let mut e = Element::new(ElementKind::Usage(u.kind), name, file, u.pos);
                        e.short_name.clone_from(&u.short_name);
                        e.visibility = u.visibility.unwrap_or(Visibility::Public);
                        let cited = (u.types.iter().map(|n| (Role::Type, n)))
                            .chain(u.subsets.iter().map(|n| (Role::Subsets, n)))
                            .chain(u.redefines.iter().map(|n| (Role::Redefines, n)));
                        (e, cited.collect(), &u.members)
                    }
                    Member::Satisfy(s) => {
                        let e = Element::new(ElementKind::Satisfy, None, file, s.pos);
                        let by = s.by.iter().map(|n| (Role::SatisfiedBy, n));
                        let cited = [(Role::Requirement, &s.requirement)].into_iter().chain(by);
                        (e, cited.collect(), &s.members)
                    }
                    Member::Verify(v) => {
                        let e = Element::new(ElementKind::Verify, None, file, v.pos);
                        (e, vec![(Role::Requirement, &v.requirement)], &v.members)
                    }
                };
            let id = self.add_element(ns, element);
            for (role, name) in cited {
                self.cite(role, file, name, Cite::Itself(id));
            }
            self.add_members(id, file, nested);
        }
    }
}

// @implements CT-MOD-002, CT-USE-007
/// Resolution of the cited names, and the errors for those that do not resolve.
impl Model {
    /// Resolves every reference and records an error for each one that does not resolve.
    fn resolve(&mut self) {
        let mut resolver = Resolver::new(self);
        let mut targets = Vec::with_capacity(self.references.len());
        let mut errors = Vec::new();
        for (index, reference) in self.references.iter().enumerate() {
            if let State::Done(Some(target)) = resolver.state[index] {
                targets.push(Some(target));
                continue;
            }
            resolver.state[index] = State::Busy;
            let outcome =
                resolver.resolve(reference.scope, &reference.name.segments, reference.skip);
            let target = outcome.as_ref().ok().copied();
            resolver.state[index] = State::Done(target);
            targets.push(target);
            if let Err(failure) = outcome {
                errors.push(self.error(reference, failure));
            }
        }
        for (reference, target) in self.references.iter_mut().zip(targets) {
            reference.target = target;
        }
        errors.sort_by(|a: &(usize, SyntaxError), b| {
            (a.0, a.1.line, a.1.column).cmp(&(b.0, b.1.line, b.1.column))
        });
        self.errors = (errors.into_iter())
            .map(|(file, error)| ModelError {
                path: self.files[file].clone(),
                error,
            })
            .collect();
    }

    fn error(&self, reference: &Reference, failure: Failure) -> (usize, SyntaxError) {
        let segments = &reference.name.segments;
        let written = segments.iter().map(|s| quote(s)).collect::<Vec<_>>();
        let noun = reference.role.noun();
        let (message, help) = match failure.segment {
            0 if segments.len() == 1 => (
                format!("cannot resolve {noun} `{}`", written[0]),
                reference.role.help(),
            ),
            0 => (
                format!(
                    "cannot resolve {noun} `{}`: no `{}` is visible here",
                    written.join("::"),
                    written[0]
                ),
                reference.role.help(),
            ),
            i => (
                format!(
                    "cannot resolve {noun} `{}`: `{}` has no visible member `{}`",
                    written.join("::"),
                    written[..i].join("::"),
                    written[i]
                ),
                "check the spelling, and that the member is public; private and protected \
                 members cannot be named from outside their namespace",
            ),
        };
        let pos = reference.name.pos;
        (
            reference.file,
            SyntaxError {
                line: pos.line,
                column: pos.column,
                message,
                help,
            },
        )
    }
}

// @implements CT-MOD-002
impl Element {
    fn new(kind: ElementKind, name: Option<String>, file: usize, pos: Pos) -> Element {
        Element {
            kind,
            name,
            short_name: None,
            visibility: Visibility::Public,
            owner: None,
            file,
            pos,
            members: Vec::new(),
            by_name: HashMap::new(),
            generals: Vec::new(),
            imports: Vec::new(),
        }
    }

    fn is_named(&self, name: &str) -> bool {
        self.name.as_deref() == Some(name) || self.short_name.as_deref() == Some(name)
    }
}

// @implements CT-MOD-002
/// Who cites a name: an element about itself, or a namespace for an import or comment.
#[derive(Clone, Copy)]
enum Cite {
    Itself(ElementId),
    Within(ElementId),
}

// @implements CT-MOD-002
/// Quotes a name segment unless it is a plain identifier.
fn quote(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if plain {
        name.to_string()
    } else {
        format!("'{name}'")
    }
}

// @implements CT-MOD-002
/// How a reference's resolution stands. `Busy` breaks cycles: a reference needed while it
/// is being resolved counts as unresolved for that inner lookup.
#[derive(Debug, Clone, Copy)]
enum State {
    Todo,
    Busy,
    Done(Option<ElementId>),
}

// @implements CT-MOD-002
/// The segment of a qualified name that did not resolve.
struct Failure {
    segment: usize,
}

// @implements CT-MOD-002
/// Which members of a namespace a lookup can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    /// From the namespace itself or one nested in it: every member.
    Inside,
    /// From a specialization: every member that is not private.
    Inherited,
    /// Through a qualified name or import: public members only.
    Outside,
}

// @implements CT-MOD-002
impl Access {
    fn sees(self, visibility: Visibility) -> bool {
        match self {
            Access::Inside => true,
            Access::Inherited => visibility != Visibility::Private,
            Access::Outside => visibility == Visibility::Public,
        }
    }
}

// @implements CT-MOD-002
/// Resolves names lazily, caching each reference, so inherited and imported namespaces are
/// resolved on first use whatever order the files declare them in.
struct Resolver<'m> {
    model: &'m Model,
    state: Vec<State>,
    /// Namespaces being searched, so inheritance and import cycles end.
    open: Vec<ElementId>,
}

// @implements CT-MOD-002
impl<'m> Resolver<'m> {
    fn new(model: &'m Model) -> Self {
        let state = (model.references.iter())
            .map(|r| match r.target {
                Some(target) => State::Done(Some(target)),
                None => State::Todo,
            })
            .collect();
        Resolver {
            model,
            state,
            open: Vec::new(),
        }
    }

    fn reference(&mut self, index: usize) -> Option<ElementId> {
        match self.state[index] {
            State::Done(target) => return target,
            State::Busy => return None,
            State::Todo => {}
        }
        self.state[index] = State::Busy;
        let r = &self.model.references[index];
        let target = self.resolve(r.scope, &r.name.segments, r.skip).ok();
        self.state[index] = State::Done(target);
        target
    }

    /// Resolves the first segment from `scope` outward, and each later one inside the last.
    fn resolve(
        &mut self,
        scope: ElementId,
        segments: &[String],
        skip: Option<ElementId>,
    ) -> Result<ElementId, Failure> {
        let mut at = Some(scope);
        let mut found = None;
        while let Some(ns) = at
            && found.is_none()
        {
            found = self.lookup(ns, &segments[0], Access::Inside, skip);
            at = self.model.element(ns).owner;
        }
        let mut current = found.ok_or(Failure { segment: 0 })?;
        for (segment, name) in segments.iter().enumerate().skip(1) {
            current =
                (self.lookup(current, name, Access::Outside, None)).ok_or(Failure { segment })?;
        }
        Ok(current)
    }

    /// Finds a member of `ns` by name: owned, then inherited, then imported.
    fn lookup(
        &mut self,
        ns: ElementId,
        name: &str,
        access: Access,
        skip: Option<ElementId>,
    ) -> Option<ElementId> {
        if self.open.contains(&ns) {
            return None;
        }
        self.open.push(ns);
        let found = (self.owned(ns, name, access, skip))
            .or_else(|| self.inherited(ns, name))
            .or_else(|| self.imported(ns, name, access));
        self.open.pop();
        found
    }

    fn owned(
        &self,
        ns: ElementId,
        name: &str,
        access: Access,
        skip: Option<ElementId>,
    ) -> Option<ElementId> {
        let candidates = self.model.element(ns).by_name.get(name)?;
        candidates
            .iter()
            .copied()
            .find(|&id| Some(id) != skip && access.sees(self.model.element(id).visibility))
    }

    fn inherited(&mut self, ns: ElementId, name: &str) -> Option<ElementId> {
        let model = self.model;
        for &general in &model.element(ns).generals {
            if let Some(g) = self.reference(general)
                && let Some(found) = self.lookup(g, name, Access::Inherited, None)
            {
                return Some(found);
            }
        }
        None
    }

    fn imported(&mut self, ns: ElementId, name: &str, access: Access) -> Option<ElementId> {
        let model = self.model;
        for import in &model.element(ns).imports {
            if access != Access::Inside && !import.public {
                continue;
            }
            let Some(target) = self.reference(import.reference) else {
                continue;
            };
            let inner = if import.all {
                Access::Inside
            } else {
                Access::Outside
            };
            let itself = model.element(target).is_named(name).then_some(target);
            let found = match import.kind {
                ImportKind::Element => itself,
                ImportKind::Members => self.lookup(target, name, inner, None),
                ImportKind::Recursive => itself.or_else(|| self.nested(target, name, inner)),
                ImportKind::MembersRecursive => self.nested(target, name, inner),
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// Finds an owned member of `ns` or of any namespace nested in it, nearest first.
    fn nested(&self, ns: ElementId, name: &str, access: Access) -> Option<ElementId> {
        let mut queue = VecDeque::from([ns]);
        while let Some(at) = queue.pop_front() {
            if let Some(found) = self.owned(at, name, access, None) {
                return Some(found);
            }
            let members = &self.model.element(at).members;
            queue.extend(
                (members.iter().copied())
                    .filter(|&m| access.sees(self.model.element(m).visibility)),
            );
        }
        None
    }
}

// @verifies CT-MOD-002, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parser::parse;

    /// Builds a model of `(path, text)` files, which must parse without errors.
    fn model(files: &[(&str, &str)]) -> Model {
        let parsed: Vec<_> = files.iter().map(|(_, text)| parse(text)).collect();
        for (p, (path, _)) in parsed.iter().zip(files) {
            assert!(p.errors.is_empty(), "{path}: {:?}", p.errors);
        }
        let inputs: Vec<(&str, &[Member])> = (files.iter().zip(&parsed))
            .map(|((path, _), p)| (*path, p.members.as_slice()))
            .collect();
        Model::build(&inputs)
    }

    fn one(text: &str) -> Model {
        model(&[("m.sysml", text)])
    }

    fn rendered(m: &Model) -> Vec<String> {
        m.errors.iter().map(ToString::to_string).collect()
    }

    /// The target of the reference that cites `name` in role `role`.
    fn target(m: &Model, role: Role, name: &str) -> Option<String> {
        let r = (m.references.iter())
            .find(|r| r.role == role && r.name.joined() == name)
            .unwrap_or_else(|| panic!("no {role:?} reference to {name}"));
        r.target.map(|t| m.qualified_name(t))
    }

    // @verifies CT-MOD-002, CT-MOD-001
    #[test]
    fn resolves_the_real_requirements_file() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../model/requirements.sysml"
        );
        let text = std::fs::read_to_string(path).unwrap();
        let m = one(&text);
        assert!(m.errors.is_empty(), "{:?}", rendered(&m));
        assert!(m.references.iter().all(|r| r.target.is_some()));
        let req = m
            .find("ClewtraceRequirements::Efficiency::installerSize")
            .unwrap();
        assert_eq!(m.element(req).short_name.as_deref(), Some("CT-EFF-001"));
        assert_eq!(
            m.find("ClewtraceRequirements::Efficiency::'CT-EFF-001'"),
            Some(req)
        );
        let phase = (m.references.iter())
            .find(|r| r.role == Role::Redefines && r.source.0 > req.0)
            .unwrap();
        assert_eq!(
            m.qualified_name(phase.target.unwrap()),
            "ClewtraceRequirements::ProductRequirement::verificationMethod"
        );
        assert_eq!(
            target(&m, Role::Type, "String"),
            Some("ScalarValues::String".into())
        );
        assert_eq!(m.files, [LIBRARY_PATH, "m.sysml"]);
    }

    // @verifies CT-MOD-002
    #[test]
    fn builds_elements_with_owners_kinds_and_positions() {
        let m = one(
            "package P {\n  private part def <'D1'> D;\n  verify requirement R;\n  \
                     requirement R { satisfy R by p; part p; }\n}\n",
        );
        let p = m.find("P").unwrap();
        let d = m.find("P::D1");
        assert_eq!(d, None, "private members are hidden from qualified names");
        let d = m.element(p).members[0];
        let e = m.element(d);
        assert_eq!(e.kind, ElementKind::Definition(Kind::Part));
        assert_eq!(e.visibility, Visibility::Private);
        assert_eq!(
            (e.owner, e.file, e.pos),
            (Some(p), 1, Pos { line: 2, column: 3 })
        );
        assert_eq!(m.element(m.element(p).members[1]).kind, ElementKind::Verify);
        let r = m.find("P::R").unwrap();
        let satisfy = m.element(r).members[0];
        assert_eq!(m.element(satisfy).kind, ElementKind::Satisfy);
        assert_eq!(m.element(Model::ROOT).kind, ElementKind::Root);
        assert_eq!(target(&m, Role::SatisfiedBy, "p"), Some("P::R::p".into()));
        assert!(m.errors.is_empty(), "{:?}", rendered(&m));
    }

    // @verifies CT-MOD-002
    #[test]
    fn unqualified_names_search_outward_and_inner_names_win() {
        let m = one(
            "package P {\n  part def T;\n  package Q {\n    part def T;\n    part a : T;\n  }\n  \
                     part b : T;\n  part c : Q::T;\n}\n",
        );
        let types: Vec<String> = (m.references.iter())
            .filter(|r| r.file == 1)
            .map(|r| m.qualified_name(r.target.unwrap()))
            .collect();
        assert_eq!(types, ["P::Q::T", "P::T", "P::Q::T"]);
    }

    // @verifies CT-MOD-002
    #[test]
    fn qualified_names_see_only_public_members() {
        let m = one(
            "package P {\n  private part def Hidden;\n  protected part def Prot;\n  \
                     part def Open;\n  part a : Hidden;\n}\npart x : P::Hidden;\npart y : P::Prot;\n\
                     part z : P::Open;\npart def S :> P::Open { part w : Open; }\n",
        );
        assert_eq!(target(&m, Role::Type, "Hidden"), Some("P::Hidden".into()));
        assert_eq!(target(&m, Role::Type, "P::Open"), Some("P::Open".into()));
        assert_eq!(
            rendered(&m),
            [
                "m.sysml:7:10: error: cannot resolve type `P::Hidden`: `P` has no visible member \
                 `Hidden`\n  help: check the spelling, and that the member is public; private \
                 and protected members cannot be named from outside their namespace",
                "m.sysml:8:10: error: cannot resolve type `P::Prot`: `P` has no visible member \
                 `Prot`\n  help: check the spelling, and that the member is public; private and \
                 protected members cannot be named from outside their namespace",
                "m.sysml:10:34: error: cannot resolve type `Open`\n  help: check the spelling, \
                 or import the package that declares it, for example `private import \
                 ScalarValues::*;`",
            ]
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn specializations_inherit_members_that_are_not_private() {
        let m = one(
            "part def A {\n  attribute x;\n  protected attribute y;\n  private attribute z;\n}\n\
                     part def B :> A;\npart b : B {\n  attribute :>> x;\n  attribute :>> y;\n  \
                     attribute :>> z;\n}\npart c subsets b { attribute :>> x; }\n",
        );
        assert_eq!(target(&m, Role::Specializes, "A"), Some("A".into()));
        let redefined: Vec<Option<String>> = (m.references.iter())
            .filter(|r| r.role == Role::Redefines)
            .map(|r| r.target.map(|t| m.qualified_name(t)))
            .collect();
        assert_eq!(
            redefined,
            [
                Some("A::x".into()),
                Some("A::y".into()),
                None,
                Some("b::x".into())
            ]
        );
        assert_eq!(
            rendered(&m),
            [
                "m.sysml:10:17: error: cannot resolve redefined feature `z`\n  help: the feature \
              must be declared in a type or definition this element is typed by or specializes, \
              or be visible from here"
            ]
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn a_name_never_resolves_to_the_element_citing_it() {
        let m = one(
            "part def T { attribute n; }\npart t : T {\n  attribute :>> n;\n  \
                     part inner : inner;\n}\n",
        );
        assert_eq!(target(&m, Role::Redefines, "n"), Some("T::n".into()));
        assert_eq!(target(&m, Role::Type, "inner"), None);
    }

    // @verifies CT-MOD-002
    #[test]
    fn imports_bring_in_elements_members_and_nested_members() {
        let m = one(
            "package L {\n  part def E;\n  package N { part def Deep; private part def Secret; }\n}\n\
                     package A {\n  import L::E;\n  import L::*;\n  import L::**;\n  part e : E;\n  \
                     part n : N::Deep;\n  part d : Deep;\n  part l : L;\n}\n\
                     package B { import L::*::**; part d : Deep; part l : L; part s : Secret; }\n\
                     package C { import all L::N::*; part s : Secret; }\n\
                     package D { import L::N; part d : N::Deep; part e : E; }\n",
        );
        let resolved: Vec<Option<String>> = (m.references.iter())
            .filter(|r| r.role == Role::Type)
            .map(|r| r.target.map(|t| m.qualified_name(t)))
            .collect();
        assert_eq!(
            resolved,
            [
                Some("L::E".into()),
                Some("L::N::Deep".into()),
                Some("L::N::Deep".into()),
                Some("L".into()),
                Some("L::N::Deep".into()),
                Some("L".into()),
                None,
                Some("L::N::Secret".into()),
                Some("L::N::Deep".into()),
                None,
            ]
        );
        let lines: Vec<u32> = m.errors.iter().map(|e| e.error.line).collect();
        assert_eq!(lines, [14, 16]);
    }

    // @verifies CT-MOD-002
    #[test]
    fn private_imports_are_not_reexported() {
        let m = one("package L { part def E; part def F; }\n\
                     package A { private import L::E; public import L::F; }\n\
                     package B { import A::*; part e : E; part f : F; part g : A::F; }\n\
                     part def G { private import L::E; }\npart def H :> G { part e : E; }\n");
        assert_eq!(target(&m, Role::Type, "F"), Some("L::F".into()));
        assert_eq!(target(&m, Role::Type, "A::F"), Some("L::F".into()));
        let lines: Vec<(u32, u32)> = (m.errors.iter())
            .map(|e| (e.error.line, e.error.column))
            .collect();
        assert_eq!(lines, [(3, 35), (5, 28)]);
    }

    // @verifies CT-MOD-002
    #[test]
    fn cycles_end_with_unresolved_names() {
        let m = one(
            "package A { public import B::*; }\npackage B { public import A::*; }\n\
                     part def X :> Y;\npart def Y :> X;\npart x : X { attribute :>> missing; }\n\
                     package S { import S::*; part def K; part k : K; }\n\
                     part def Z :> Z::m { attribute m; }\npart q : A::nothing;\n",
        );
        assert_eq!(target(&m, Role::Type, "K"), Some("S::K".into()));
        assert_eq!(target(&m, Role::Specializes, "Y"), Some("Y".into()));
        assert_eq!(target(&m, Role::Specializes, "X"), Some("X".into()));
        let messages: Vec<&str> = m.errors.iter().map(|e| e.error.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "cannot resolve redefined feature `missing`",
                "cannot resolve specialized definition `Z::m`: no `Z` is visible here",
                "cannot resolve type `A::nothing`: `A` has no visible member `nothing`",
            ]
        );
    }

    // @verifies CT-MOD-002, CT-USE-007
    #[test]
    fn reports_each_role_with_file_line_and_column() {
        let m = model(&[
            (
                "b.sysml",
                "package B {\n  import Nope::*;\n  part def D :> Missing;\n  \
                 comment about Gone /* c */\n  satisfy Req by Who;\n  verify Ver;\n  \
                 part p : D subsets Sub;\n  part q : 'odd name'::x;\n}\n",
            ),
            ("a.sysml", "part def A :> B::D;\npart a : B::Gone;\n"),
        ]);
        assert_eq!(target(&m, Role::Specializes, "B::D"), Some("B::D".into()));
        let found: Vec<String> = (m.errors.iter())
            .map(|e| {
                format!(
                    "{}:{}:{}: {}",
                    e.path, e.error.line, e.error.column, e.error.message
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                "b.sysml:2:10: cannot resolve import target `Nope`",
                "b.sysml:3:17: cannot resolve specialized definition `Missing`",
                "b.sysml:4:17: cannot resolve annotated element `Gone`",
                "b.sysml:5:11: cannot resolve requirement `Req`",
                "b.sysml:5:18: cannot resolve satisfying element `Who`",
                "b.sysml:6:10: cannot resolve requirement `Ver`",
                "b.sysml:7:22: cannot resolve subsetted feature `Sub`",
                "b.sysml:8:12: cannot resolve type `'odd name'::x`: no `'odd name'` is visible here",
                "a.sysml:2:10: cannot resolve type `B::Gone`: `B` has no visible member `Gone`",
            ]
        );
        assert_eq!(
            m.errors[0].error.help,
            "check the spelling and that the file declaring it is in the project; only \
             ScalarValues of the standard library is built in"
        );
    }

    // @verifies CT-MOD-002
    #[test]
    fn finds_and_names_elements() {
        let m = one("package 'My Pkg' { part def <'S1'> 'a b'; part <'U1'>; part def _x1; }\n");
        let ab = m.find("'My Pkg'::'a b'").unwrap();
        assert_eq!(m.find("'My Pkg'::S1"), Some(ab));
        assert_eq!(m.qualified_name(ab), "'My Pkg'::'a b'");
        assert_eq!(
            m.qualified_name(m.find("'My Pkg'::U1").unwrap()),
            "'My Pkg'::U1"
        );
        assert_eq!(
            m.qualified_name(m.find("'My Pkg'::_x1").unwrap()),
            "'My Pkg'::_x1"
        );
        assert_eq!(m.find("'My Pkg'::missing"), None);
        assert_eq!(m.find(""), None);
        assert_eq!(m.qualified_name(Model::ROOT), "");
        let unnamed = one("part { part y; }\n");
        let outer = unnamed.element(Model::ROOT).members[1];
        let y = unnamed.element(outer).members[0];
        assert_eq!(unnamed.qualified_name(y), "(unnamed)::y");
    }

    // @verifies CT-MOD-002
    #[test]
    fn the_built_in_library_parses_and_resolves() {
        let parsed = parse(LIBRARY);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let m = one("");
        assert!(m.errors.is_empty());
        let positive = m.find("ScalarValues::Positive").unwrap();
        assert_eq!(m.element(positive).file, 0);
        assert!(m.references.iter().all(|r| r.target.is_some()));
    }
}
