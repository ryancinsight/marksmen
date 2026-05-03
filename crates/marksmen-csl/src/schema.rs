use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename = "style")]
pub struct Style {
    #[serde(rename = "@class")]
    pub class: String,
    #[serde(rename = "@version")]
    pub version: String,
    #[serde(default)]
    pub info: Option<Info>,
    #[serde(rename = "locale", default)]
    pub locales: Vec<Locale>,
    #[serde(rename = "macro", default)]
    pub macros: Vec<Macro>,
    pub citation: Citation,
    #[serde(default)]
    pub bibliography: Option<Bibliography>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Info {
    pub title: String,
    pub id: String,
    pub updated: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Locale {
    #[serde(rename = "@xml:lang")]
    pub lang: Option<String>,
    #[serde(rename = "terms", default)]
    pub terms: Option<Terms>,
    #[serde(rename = "date", default)]
    pub dates: Vec<DateDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Terms {
    #[serde(rename = "term", default)]
    pub items: Vec<Term>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Term {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@form")]
    pub form: Option<String>,
    #[serde(rename = "single", default)]
    pub single: Option<String>,
    #[serde(rename = "multiple", default)]
    pub multiple: Option<String>,
    #[serde(rename = "$value", default)]
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Macro {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "$value", default)]
    pub elements: Vec<RenderingElement>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Citation {
    #[serde(default)]
    pub layout: Option<Layout>,
    #[serde(default)]
    pub sort: Option<Sort>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Bibliography {
    #[serde(default)]
    pub layout: Option<Layout>,
    #[serde(default)]
    pub sort: Option<Sort>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Layout {
    #[serde(rename = "@prefix")]
    pub prefix: Option<String>,
    #[serde(rename = "@suffix")]
    pub suffix: Option<String>,
    #[serde(rename = "@delimiter")]
    pub delimiter: Option<String>,
    #[serde(rename = "$value", default)]
    pub elements: Vec<RenderingElement>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Sort {
    #[serde(rename = "key", default)]
    pub keys: Vec<SortKey>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SortKey {
    #[serde(rename = "@variable")]
    pub variable: Option<String>,
    #[serde(rename = "@macro")]
    pub macro_name: Option<String>,
    #[serde(rename = "@sort")]
    pub sort: Option<String>, // "ascending" | "descending"
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum RenderingElement {
    Text(Text),
    Date(DateDef),
    Number(Number),
    Names(Names),
    Label(Label),
    Group(Group),
    Choose(Choose),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Text {
    #[serde(rename = "@variable")]
    pub variable: Option<String>,
    #[serde(rename = "@macro")]
    pub macro_name: Option<String>,
    #[serde(rename = "@term")]
    pub term: Option<String>,
    #[serde(rename = "@value")]
    pub value: Option<String>,
    #[serde(rename = "@prefix")]
    pub prefix: Option<String>,
    #[serde(rename = "@suffix")]
    pub suffix: Option<String>,
    #[serde(rename = "@quotes")]
    pub quotes: Option<bool>,
    #[serde(rename = "@font-style")]
    pub font_style: Option<String>,
    #[serde(rename = "@font-weight")]
    pub font_weight: Option<String>,
    #[serde(rename = "@text-decoration")]
    pub text_decoration: Option<String>,
    #[serde(rename = "@vertical-align")]
    pub vertical_align: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DateDef {
    #[serde(rename = "@variable")]
    pub variable: String,
    #[serde(rename = "@form")]
    pub form: Option<String>,
    #[serde(rename = "@date-parts")]
    pub date_parts: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Number {
    #[serde(rename = "@variable")]
    pub variable: String,
    #[serde(rename = "@form")]
    pub form: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Names {
    #[serde(rename = "@variable")]
    pub variable: String,
    #[serde(rename = "name")]
    pub name: Option<Name>,
    #[serde(rename = "label")]
    pub label: Option<Label>,
    #[serde(rename = "substitute")]
    pub substitute: Option<Substitute>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Name {
    #[serde(rename = "@form")]
    pub form: Option<String>,
    #[serde(rename = "@name-as-sort-order")]
    pub name_as_sort_order: Option<String>,
    #[serde(rename = "@and")]
    pub and: Option<String>,
    #[serde(rename = "@delimiter")]
    pub delimiter: Option<String>,
    #[serde(rename = "@delimiter-precedes-last")]
    pub delimiter_precedes_last: Option<String>,
    #[serde(rename = "@et-al-min")]
    pub et_al_min: Option<usize>,
    #[serde(rename = "@et-al-use-first")]
    pub et_al_use_first: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Substitute {
    #[serde(rename = "$value", default)]
    pub elements: Vec<RenderingElement>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Label {
    #[serde(rename = "@variable")]
    pub variable: String,
    #[serde(rename = "@form")]
    pub form: Option<String>,
    #[serde(rename = "@plural")]
    pub plural: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Group {
    #[serde(rename = "@delimiter")]
    pub delimiter: Option<String>,
    #[serde(rename = "$value", default)]
    pub elements: Vec<RenderingElement>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Choose {
    #[serde(rename = "if", default)]
    pub if_block: Vec<IfBlock>,
    #[serde(rename = "else-if", default)]
    pub else_if_block: Vec<IfBlock>,
    #[serde(rename = "else")]
    pub else_block: Option<ElseBlock>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IfBlock {
    #[serde(rename = "@type")]
    pub type_match: Option<String>,
    #[serde(rename = "@variable")]
    pub variable_match: Option<String>,
    #[serde(rename = "@is-numeric")]
    pub is_numeric: Option<String>,
    #[serde(rename = "@is-uncertain-date")]
    pub is_uncertain_date: Option<String>,
    #[serde(rename = "@match")]
    pub match_condition: Option<String>, // "any" | "all" | "none"
    #[serde(rename = "$value", default)]
    pub elements: Vec<RenderingElement>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ElseBlock {
    #[serde(rename = "$value", default)]
    pub elements: Vec<RenderingElement>,
}
