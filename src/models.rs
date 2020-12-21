use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub jobs: Vec<Job>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Job {
    pub name: String,
    pub repo_url: String,
    pub branch: String,
    pub build_script: String,
    pub poll_interval_seconds: u64,
    pub auth: Option<GitAuth>,
}

impl Job {
    pub fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum GitAuth {
    PrivateKey {
        path: String,
        passphrase: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum JobEvent {
    Log {
        job: String,
        line: String,
        is_stderr: bool,
    },
}
