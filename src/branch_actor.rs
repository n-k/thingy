use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{create_dir_all, remove_dir_all, File},
    io::{BufRead, BufReader},
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
    pub fn new(job: Job, branch: String, dir: PathBuf, last_seen_commit: Option<String>) -> Self {
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

    fn start_build(
        &mut self,
        _ctx: &mut Context<Self>,
        hash: Option<String>,
    ) -> Result<(), std::io::Error> {
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
            hash.clone(),
            &checkout_dir,
            self.job.auth.as_ref(),
        ) {
            let h = BuildActor::new(
                self.job.build_script.clone(),
                checkout_dir.clone(),
                hash.clone(),
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
        if hash.is_some() {
            self.state.last_seen_commit = hash.clone();
        }
        let build = BuildDetails {
            build_num: bn,
            commit_hash: hash,
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
#[rtype(result = "Result<(), std::io::Error>")]
pub struct NewCommitMsg(pub String);

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct BuildStoppedMsg {
    pub build_num: u64,
    pub status: String,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<BranchDetails, std::io::Error>")]
pub struct GetBranchDetailsMsg;

#[derive(Debug, Clone)]
struct BuildLink {
    build_num: u64,
    addr: Addr<BuildActor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchDetails {
    last_seen_commit: Option<String>,
    builds: Vec<BuildDetails>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildDetails {
    build_num: u64,
    commit_hash: Option<String>,
    status: String,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<LogResponse, std::io::Error>")]
pub struct GetBuildLogLinesMsg {
    pub build_num: u64,
    pub start: u32,
    pub num_lines: u32,
}
#[derive(Debug, Serialize)]
pub struct LogResponse {
    pub lines: Vec<String>,
    pub has_more: bool,
    pub status: Option<String>,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<Option<Addr<BuildActor>>, std::io::Error>")]
pub struct GetBuildActorMsg(pub u64);

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct BuildNowMsg;

impl Handler<BuildNowMsg> for BranchActor {
    type Result = Result<(), std::io::Error>;

    fn handle(&mut self, _msg: BuildNowMsg, ctx: &mut Self::Context) -> Self::Result {
        self.start_build(ctx, None)?;
        Ok(())
    }
}

impl Handler<NewCommitMsg> for BranchActor {
    type Result = Result<(), std::io::Error>;

    fn handle(&mut self, msg: NewCommitMsg, ctx: &mut Self::Context) -> Self::Result {
        let hash = Some(msg.0.clone());
        if !self.state.last_seen_commit.eq(&hash) {
            self.start_build(ctx, hash)?;
        }
        Ok(())
    }
}

impl Handler<BuildStoppedMsg> for BranchActor {
    type Result = Result<(), std::io::Error>;

    fn handle(&mut self, msg: BuildStoppedMsg, _ctx: &mut Self::Context) -> Self::Result {
        self.builds = self
            .builds
            .clone()
            .into_iter()
            .filter(|b| b.build_num != msg.build_num)
            .collect();
        self.state
            .builds
            .iter_mut()
            .filter(|b| b.build_num == msg.build_num)
            .for_each(|b| {
                b.status = msg.status.clone();
            });
        self.write_data_file()?;
        Ok(())
    }
}

impl Handler<GetBranchDetailsMsg> for BranchActor {
    type Result = Result<BranchDetails, std::io::Error>;

    fn handle(&mut self, _msg: GetBranchDetailsMsg, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.state.clone())
    }
}

impl Handler<GetBuildLogLinesMsg> for BranchActor {
    type Result = Result<LogResponse, std::io::Error>;

    fn handle(&mut self, _msg: GetBuildLogLinesMsg, _ctx: &mut Self::Context) -> Self::Result {
        let log_file = self.dir.join(format!("{}", _msg.build_num)).join("log.txt");
        if log_file.exists() {
            let file = File::open(&log_file)?;
            let reader = BufReader::new(file);

            let lines = reader
                .lines()
                .filter(|l| l.is_ok())
                .map(|l| l.unwrap())
                .skip(_msg.start as usize);
            let mut batch: Vec<String> = lines.take(_msg.num_lines as usize + 1).collect();
            let has_more = batch.len() >= _msg.num_lines as usize;
            if has_more {
                for _ in batch.drain(_msg.num_lines as usize..) {}
            }
            let status = self
                .state
                .builds
                .iter()
                .find(|b| b.build_num == _msg.build_num)
                .map(|b| b.status.clone());
            return Ok(LogResponse {
                lines: batch,
                has_more,
                status,
            });
        }
        Ok(LogResponse {
            lines: vec![],
            has_more: false,
            status: None,
        })
    }
}

impl Handler<GetBuildActorMsg> for BranchActor {
    type Result = Result<Option<Addr<BuildActor>>, std::io::Error>;

    fn handle(&mut self, msg: GetBuildActorMsg, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self
            .builds
            .iter()
            .find(|l| l.build_num == msg.0)
            .map(|a| a.addr.clone()))
    }
}
