use actix::prelude::*;
use std::{
    fs::{create_dir_all, remove_dir_all},
    path::PathBuf,
};

use crate::{build_actor::BuildActor, git_utils::clone_commit, models::Job};

#[derive(Debug)]
pub struct BranchActor {
    pub job: Job,
    pub branch: String,
    pub dir: PathBuf,
    pub last_seen_commit: String,
    pub builds: Vec<Addr<BuildActor>>,
}

impl BranchActor {
    pub fn new(job: Job, branch: String, dir: PathBuf, last_seen_commit: String) -> Self {
        BranchActor {
            job,
            branch,
            dir,
            last_seen_commit,
            builds: vec![],
        }
    }
}

impl Actor for BranchActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        println!("Branch actor started");
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        println!("Branch actor stopped");
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<BranchResponse, std::io::Error>")]
pub enum BranchMessage {
    NewCommit { hash: String },
    BuildStopped { addr: Addr<BuildActor> },
}

#[derive(Debug)]
pub enum BranchResponse {
    Ack,
}

impl Handler<BranchMessage> for BranchActor {
    type Result = Result<BranchResponse, std::io::Error>;

    fn handle(&mut self, _msg: BranchMessage, _ctx: &mut Context<Self>) -> Self::Result {
        println!("Message received: {:#?}", _msg);

        match _msg {
            BranchMessage::NewCommit { hash } => {
                if !self.last_seen_commit.eq(&hash) {
                    // start a build, update last_seen
                    self.last_seen_commit = hash.clone();
                    let build_dir = self.dir.join(&hash);
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
                        &hash,
                        &checkout_dir,
                        self.job.auth.as_ref(),
                    ) {
                        let h = BuildActor::new(
                            self.job.build_script.clone(),
                            checkout_dir.clone(),
                            Some(hash.clone()),
                            _ctx.address(),
                        )
                        .start();
                        self.builds.push(h);
                    }
                }
            }
            BranchMessage::BuildStopped { addr } => {
                self.builds = self
                    .builds
                    .clone()
                    .into_iter()
                    .filter(|b| !b.eq(&addr))
                    .collect();
            }
        }

        Ok(BranchResponse::Ack)
    }
}
