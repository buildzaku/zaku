use std::borrow::Cow;

pub type QueryFieldAccessor = fn(&mut LanguageQueries) -> &mut Option<Cow<'static, str>>;

pub const QUERY_FILENAME_PREFIXES: &[(&str, QueryFieldAccessor)] = &[
    ("highlights", |queries| &mut queries.highlights),
    ("brackets", |queries| &mut queries.brackets),
    ("indents", |queries| &mut queries.indents),
    ("injections", |queries| &mut queries.injections),
    ("overrides", |queries| &mut queries.overrides),
    ("redactions", |queries| &mut queries.redactions),
];

#[derive(Debug, Default)]
pub struct LanguageQueries {
    pub highlights: Option<Cow<'static, str>>,
    pub brackets: Option<Cow<'static, str>>,
    pub indents: Option<Cow<'static, str>>,
    pub injections: Option<Cow<'static, str>>,
    pub overrides: Option<Cow<'static, str>>,
    pub redactions: Option<Cow<'static, str>>,
}
