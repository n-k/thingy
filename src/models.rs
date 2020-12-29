use serde::{Serialize, Deserialize};

/// A workspace containing build jobs
#[derive(Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub jobs: Vec<Job>,
}

/// A build job
#[derive(Serialize, Deserialize, Clone)]
pub struct Job {
    /// name of the job, must be unique within a workspace
    pub name: String,
    /// Git fetch URL
    pub repo_url: String,
    /// DEPRECATED: kept for backwards compatibility with v0.1.x
    /// Setting 'branch="abcde"' is equivalent to saying 'branches=["abcde"]' and 'ignore_branches=None'
    pub branch: Option<String>,
    /// Which branches to build, omit to build all
    pub branches: Option<Vec<String>>,
    /// Which branches to ignore, omit to ignore none
    pub ignore_branches: Option<Vec<String>>,
    /// Path to script in repository which will be called to build
    pub build_script: String,
    /// Interval in seconds to wait before polling for changes
    pub poll_interval_seconds: u64,
    /// Authentication for Git fetch, if required
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
