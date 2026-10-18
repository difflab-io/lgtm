#[derive(Clone, Debug, Default)]
pub struct SessionFilter {
    pub path: Option<String>,
    pub git_dir: Option<String>,
    pub completed: bool,
}
