use std::{collections::HashMap, fs::create_dir_all, path::PathBuf, time::Duration};

use crate::{
    branch_actor::{BranchActor, BranchMessage},
    git_utils::get_branch_hashes,
    models::*,
};
use actix::prelude::*;
use serde::Serialize;

#[derive(Debug)]
pub struct JobActor {
    pub job: Job,
    pub dir: PathBuf,
    pub branch_actors: HashMap<String, Addr<BranchActor>>,
}

impl JobActor {
    pub fn new(job: Job, dir: PathBuf) -> Self {
        JobActor {
            job,
            dir,
            branch_actors: HashMap::new(),
        }
    }

    /// poll branches for a job
    fn _poll(&mut self, context: &mut Context<Self>) {
        context.address().do_send(JobMessage::Poll);
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<JobResponse, std::io::Error>")]
pub enum JobMessage {
    Poll,
    GetDetails,
    GetBranchActor(String),
}

#[derive(Debug)]
pub enum JobResponse {
    Ack,
    JobDetails(JobDetailsResponse),
    Branch { addr: Option<Addr<BranchActor>> },
}

#[derive(Debug, Serialize)]
pub struct JobDetailsResponse {
    name: String,
    branches: Vec<String>,
}

impl Actor for JobActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        if let Some(i) = self.job.poll_interval_seconds {
            _ctx.run_interval(Duration::from_secs(i), Self::_poll);
        }
        _ctx.notify(JobMessage::Poll);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {}
}

impl Handler<JobMessage> for JobActor {
    type Result = Result<JobResponse, std::io::Error>;

    fn handle(&mut self, _msg: JobMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match _msg {
            JobMessage::Poll => {
                if let Ok(hashes) = get_branch_hashes(&self.job.repo_url, self.job.auth.as_ref()) {
                    for (k, v) in hashes.iter() {
                        match self.branch_actors.get(k) {
                            Some(a) => {
                                a.do_send(BranchMessage::NewCommit { hash: v.clone() });
                            }
                            _ => {
                                // ensure dir
                                let bpath = self.dir.join(k);
                                create_dir_all(&bpath)?;
                                let h =
                                    BranchActor::new(self.job.clone(), k.clone(), bpath, "".into())
                                        .start();
                                self.branch_actors.insert(k.clone(), h);
                                self.branch_actors
                                    .get(k)
                                    .unwrap()
                                    .do_send(BranchMessage::NewCommit { hash: v.clone() });
                            }
                        }
                    }
                    // remove branches which are no longer present
                    self.branch_actors = self
                        .branch_actors
                        .clone()
                        .into_iter()
                        .filter(|(k, _)| hashes.contains_key(k))
                        .collect();
                }
                Ok(JobResponse::Ack)
            }
            JobMessage::GetDetails => {
                let branches: Vec<String> =
                    self.branch_actors.iter().map(|(k, _)| k.clone()).collect();
                Ok(JobResponse::JobDetails(JobDetailsResponse {
                    name: self.job.name.clone(),
                    branches,
                }))
            }
            JobMessage::GetBranchActor(b) => {
                let addr = self.branch_actors
                    .get(&b)
                    .map(|a| a.clone());
                Ok(JobResponse::Branch { addr })
            }
        }
    }
}
