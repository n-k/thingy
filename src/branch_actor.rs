use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{create_dir_all, remove_dir_all},
    path::PathBuf,
};

use crate::{build_actor::BuildActor, git_utils::clone_commit, models::Job};

#[derive(Debug)]
pub struct BranchActor {
    job: Job,
    branch: String,
    dir: PathBuf,
    builds: Vec<BuildLink>,
    state: BranchDetails,
}

impl BranchActor {
    pub fn new(job: Job, branch: String, dir: PathBuf, last_seen_commit: String) -> Self {
        BranchActor {
            job,
            branch,
            dir,
            builds: vec![],
            state: BranchDetails {
                last_seen_commit,
                builds: vec![],
            },
        }
    }

    fn inc_build_num(&mut self) -> Result<u64, std::io::Error> {
        let build_num_file = self.dir.join("build_num.txt");
        let next_num: u64 = if build_num_file.exists() {
            let n: u64 = std::fs::read_to_string(&build_num_file)?
                .parse()
                .unwrap_or_default();
            n + 1
        } else {
            1
        };
        std::fs::write(build_num_file, format!("{}", next_num).as_bytes())?;
        Ok(next_num)
    }

    fn start_build(&mut self, _ctx: &mut Context<Self>, hash: &str) -> Result<(), std::io::Error> {
        let bn = self.inc_build_num()?;
        // start a build, update last_seen
        let build_dir = self.dir.join(&format!("{}", bn));
        if build_dir.exists() {
            remove_dir_all(&build_dir)?;
        }
        create_dir_all(&build_dir)?;
        let checkout_dir = build_dir.join("repo");
        create_dir_all(&checkout_dir)?;
        // do build
        if let Ok(_) = clone_commit(
            &self.job.repo_url,
            &self.branch,
            hash,
            &checkout_dir,
            self.job.auth.as_ref(),
        ) {
            let h = BuildActor::new(
                self.job.build_script.clone(),
                checkout_dir.clone(),
                Some(hash.to_string()),
                _ctx.address(),
                build_dir.join("log.txt"),
                bn,
            )
            .start();
            self.builds.push(BuildLink {
                build_num: bn,
                addr: h,
            });
        }
        self.state.last_seen_commit = hash.to_string();
        let build = BuildDetails {
            build_num: bn,
            commit_hash: hash.to_string(),
            status: "building".into(),
        };
        self.state.builds.push(build);
        self.write_data_file()?;
        Ok(())
    }

    fn get_data_path(&self) -> PathBuf {
        self.dir.join("data.json")
    }

    fn write_data_file(&self) -> Result<(), std::io::Error> {
        std::fs::write(self.get_data_path(), serde_json::to_string(&self.state)?)?;
        Ok(())
    }
}

impl Actor for BranchActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        // create build num and data files
        let build_num_path = self.dir.join("build_num.txt");
        if !build_num_path.exists() {
            std::fs::write(build_num_path, "0".as_bytes()).unwrap();
        }
        let data_path = self.get_data_path();
        if !data_path.exists() {
            self.write_data_file().unwrap();
        } else {
            let det: BranchDetails =
                serde_json::from_str(std::fs::read_to_string(data_path).unwrap().as_str()).unwrap();
            self.state = det;
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {}
}

#[derive(Message, Debug)]
#[rtype(result = "Result<BranchResponse, std::io::Error>")]
pub enum BranchMessage {
    NewCommit { hash: String },
    BuildStopped { build_num: u64, status: String },
    GetDetails,
}

#[derive(Debug)]
pub enum BranchResponse {
    Ack,
    Details(BranchDetails),
}

#[derive(Debug, Clone)]
struct BuildLink {
    build_num: u64,
    addr: Addr<BuildActor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchDetails {
    last_seen_commit: String,
    builds: Vec<BuildDetails>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildDetails {
    build_num: u64,
    commit_hash: String,
    status: String,
}

impl Handler<BranchMessage> for BranchActor {
    type Result = Result<BranchResponse, std::io::Error>;

    fn handle(&mut self, _msg: BranchMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match _msg {
            BranchMessage::NewCommit { hash } => {
                if !self.state.last_seen_commit.eq(&hash) {
                    self.start_build(_ctx, hash.as_str())?;
                }
                Ok(BranchResponse::Ack)
            }
            BranchMessage::BuildStopped { build_num, status } => {
                self.builds = self
                    .builds
                    .clone()
                    .into_iter()
                    .filter(|b| b.build_num != build_num)
                    .collect();
                self.state
                    .builds
                    .iter_mut()
                    .filter(|b| b.build_num == build_num)
                    .for_each(|b| {
                        b.status = status.clone();
                    });
                self.write_data_file()?;
                Ok(BranchResponse::Ack)
            }
            BranchMessage::GetDetails => {
                Ok(BranchResponse::Details(self.state.clone()))
            }
        }
    }
}
