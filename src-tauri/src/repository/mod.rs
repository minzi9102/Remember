use crate::application::config::RuntimeMode;

#[derive(Debug, Clone)]
pub struct RepositoryLayer {
    runtime_mode: RuntimeMode,
}

impl RepositoryLayer {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        Self { runtime_mode }
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }
}
