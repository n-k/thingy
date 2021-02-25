use std::{collections::HashMap, fs::create_dir_all, path::PathBuf, time::Duration};

use crate::{
    branch_actor::{BranchActor, NewCommitMsg},
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
        context.address().do_send(JobPollMsg);
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct JobPollMsg;

#[derive(Message, Debug)]
#[rtype(result = "Result<JobDetailsResponse, std::io::Error>")]
pub struct GetJobDetailsMsg;

#[derive(Debug, Serialize)]
pub struct JobDetailsResponse {
    name: String,
    branches: Vec<String>,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<Option<Addr<BranchActor>>, std::io::Error>")]
pub struct GetBranchActorMsg(pub String);

impl Actor for JobActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        if let Some(i) = self.job.poll_interval_seconds {
            _ctx.run_interval(Duration::from_secs(i), Self::_poll);
        }
        _ctx.notify(JobPollMsg);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {}
}

impl Handler<JobPollMsg> for JobActor {
    type Result = Result<(), std::io::Error>;

    fn handle(&mut self, _msg: JobPollMsg, _ctx: &mut Self::Context) -> Self::Result {
        if let Ok(hashes) = get_branch_hashes(&self.job.repo_url, self.job.auth.as_ref()) {
            for (k, v) in hashes.iter() {
                match self.branch_actors.get(k) {
                    Some(a) => {
                        a.do_send(NewCommitMsg(v.clone()));
                    }
                    _ => {
                        // ensure dir
                        let bpath = self.dir.join(k);
                        create_dir_all(&bpath)?;
                        let h =
                            BranchActor::new(self.job.clone(), k.clone(), bpath, None).start();
                        self.branch_actors.insert(k.clone(), h);
                        self.branch_actors
                            .get(k)
                            .unwrap()
                            .do_send(NewCommitMsg(v.clone()));
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
        Ok(())
    }
}

impl Handler<GetJobDetailsMsg> for JobActor {
    type Result = Result<JobDetailsResponse, std::io::Error>;

    fn handle(&mut self, _msg: GetJobDetailsMsg, _ctx: &mut Self::Context) -> Self::Result {
        let branches: Vec<String> = self.branch_actors.iter().map(|(k, _)| k.clone()).collect();
        Ok(JobDetailsResponse {
            name: self.job.name.clone(),
            branches,
        })
    }
}

impl Handler<GetBranchActorMsg> for JobActor {
    type Result = Result<Option<Addr<BranchActor>>, std::io::Error>;

    fn handle(&mut self, msg: GetBranchActorMsg, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.branch_actors.get(&msg.0).map(|a| a.clone()))
    }
}
