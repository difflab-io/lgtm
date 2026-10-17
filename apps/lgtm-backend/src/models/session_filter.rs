#[derive(Clone, Debug, Default)]
pub struct SessionFilter {
    pub path: Option<String>,
    pub repository: Option<String>,
    pub completed: bool,
}
