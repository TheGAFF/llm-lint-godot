#[derive(Clone, Copy)]
pub(crate) struct NativeFinding<'a> {
    pub rule: &'a str,
    pub level: &'a str,
    pub path: &'a str,
    pub line: Option<i64>,
    pub column: Option<i64>,
}
