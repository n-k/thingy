use std::{collections::HashMap, fs::create_dir_all, path::PathBuf, time::Duration};

use crate::{
    branch_actor::{BranchActor, BranchMessage},
    git_utils::get_branch_hashes,
    models::*,
};
use actix::prelude::*;

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
    /// owns and keeps branch actors in sync
    fn _poll(&mut self, context: &mut Context<Self>) {
        context.address().do_send(JobMessage::Poll);
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<JobResponse, std::io::Error>")]
pub enum JobMessage {
    Poll,
}

#[derive(Debug)]
pub enum JobResponse {
    Ack,
}

impl Actor for JobActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        println!("Job actor started");
        if let Some(i) = self.job.poll_interval_seconds {
            _ctx.run_interval(Duration::from_secs(i), Self::_poll);
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        println!("Job actor stopped");
    }
}

impl Handler<JobMessage> for JobActor {
    type Result = Result<JobResponse, std::io::Error>;

    fn handle(&mut self, _msg: JobMessage, _ctx: &mut Context<Self>) -> Self::Result {
        println!("Message received: {:#?}", _msg);

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
                                // todo: keep track of last seen state in file in branch dir
                                let h = BranchActor::new(
                                    self.job.clone(),
                                    k.clone(),
                                    bpath,
                                    "".into(),
                                )
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
            }
        }

        Ok(JobResponse::Ack)
    }
}
