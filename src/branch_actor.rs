use actix::prelude::*;
use std::{
    fs::{create_dir_all, remove_dir_all},
    path::PathBuf,
};

use crate::{git_utils::clone_commit, models::GitAuth};

#[derive(Debug)]
pub struct BranchActor {
    pub git_url: String,
    pub branch: String,
    pub dir: PathBuf,
    pub auth: Option<GitAuth>,
    pub last_seen_commit: String,
}

impl BranchActor {
    pub fn new(
        git_url: String,
        branch: String,
        dir: PathBuf,
        auth: Option<GitAuth>,
        last_seen_commit: String,
    ) -> Self {
        BranchActor {
            git_url,
            branch,
            dir,
            auth,
            last_seen_commit,
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
                    let build_dir = self.dir.join(&hash);
                    if build_dir.exists() {
                        remove_dir_all(&build_dir)?;
                    }
                    create_dir_all(&build_dir)?;
                    let checkout_dir = build_dir.join("repo");
                    // do build
                    if let Ok(_) = clone_commit(&self.git_url, &self.branch, &hash, &checkout_dir, self.auth.as_ref()) {
                        self.last_seen_commit = hash;
                    }
                }
            }
        }

        Ok(BranchResponse::Ack)
    }
}
