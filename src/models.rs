use std::{collections::HashSet, path::PathBuf};

use serde::{Deserialize, Serialize};

/// A workspace containing build jobs
#[derive(Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub jobs: Vec<Job>,
}

impl Workspace {
    pub fn from_dir_path(
        path: &PathBuf,
    ) -> std::result::Result<Workspace, Box<dyn std::error::Error>> {
        println!("Initing thingy in workspace {:?}", &path);

        let ws_yaml_path = path.clone().join("thingy.yaml");

        let md = std::fs::metadata(&ws_yaml_path);
        if let Err(err) = &md {
            return Err(format!(
                "Could not read config from {:?}. Exiting. Does the file exist? Error: {:?}",
                &ws_yaml_path, &err
            )
            .into());
        }
        let md = md.unwrap();
        if !md.is_file() {
            return Err(format!("{:?} is not a regular file. Exiting.", &ws_yaml_path).into());
        }
        let contents = std::fs::read_to_string(&ws_yaml_path);
        if let Err(err) = &contents {
            return Err(format!(
                "Could not read {:?}. Exiting. Error: {:?}",
                &ws_yaml_path, &err
            )
            .into());
        }
        let contents = contents.unwrap();
        let ws = serde_yaml::from_str::<Workspace>(&contents);

        if let Err(err) = &ws {
            return Err(format!(
                "Could not read {:?}. Exiting. Does the file contain valid YAML? Error: {:?}",
                &ws_yaml_path, &err
            )
            .into());
        }

        let mut ws = ws.unwrap();
        let names: Vec<&str> = ws.jobs.iter().map(|j| j.name.trim()).collect();

        let mut uniq = HashSet::<&str>::new();
        for n in names {
            if n.is_empty() {
                return Err("Found job with empty name. Exiting.".into());
            }
            if uniq.contains(n) {
                return Err(format!("Workspace config contains duplicate jobs with name '{}'. Note that names are trimmed when read. Exiting.", n).into());
            }
            uniq.insert(n);
        }

        for j in &mut ws.jobs {
            if let Err(err) = &j.validate() {
                return Err(
                    format!("Configuration for {} is invalid: {}. Exiting.", j.name, err).into(),
                );
            }
        }

        // ensure job dirs
        for j in &ws.jobs {
            let name = j.name.trim();
            let dir = path.join(name);

            if dir.is_file() {
                return Err(format!("{:?} is a file. Expected directory or nothing.", &dir).into());
            }

            if !dir.exists() {
                if let Err(err) = std::fs::create_dir_all(&dir) {
                    return Err(format!(
                        "Could not create job dir {:?}. Exiting. Error: {:?}",
                        &dir, &err
                    )
                    .into());
                }
            }
        }

        Ok(ws)
    }
}

/// A build job
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Job {
    /// name of the job, must be unique within a workspace
    pub name: String,
    /// Git fetch URL
    pub repo_url: String,
    /// Path to script in repository which will be called
    pub build_script: String,
    /// Interval in seconds to wait before polling for changes
    pub poll_interval_seconds: Option<u64>,
    /// Authentication for Git fetch, if required
    pub auth: Option<GitAuth>,
}

impl Job {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.repo_url.trim().is_empty() {
            return Err("Repository url is empty.".into());
        }

        if self.build_script.trim().is_empty() {
            return Err("Build script path is empty.".into());
        }

        if self.poll_interval_seconds.eq(&Some(0)) {
            return Err("Poll interval must be > 0.".into());
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GitAuth {
    PrivateKey {
        path: String,
        passphrase: Option<String>,
    },
    UserPass {
        username: String,
        password: String,
    },
}
