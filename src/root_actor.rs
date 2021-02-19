use std::{collections::HashMap, path::PathBuf};

use crate::{job_actor::JobActor, models::*};
use actix::prelude::*;

pub struct Thingy {
    pub workpace: Workspace,
    pub dir: PathBuf,
    pub job_actors: HashMap<String, Addr<JobActor>>,
}

impl Thingy {
    pub fn new(workpace: Workspace, dir: PathBuf) -> Self {
        Thingy {
            workpace,
            dir,
            job_actors: HashMap::new(),
        }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<GetJobResponse, std::io::Error>")]
pub struct GetJobsMsg;
#[derive(Debug)]
pub struct GetJobResponse(pub Vec<Job>);

#[derive(Message, Debug)]
#[rtype(result = "Result<GetJobActorResponse, std::io::Error>")]
pub struct GetJobActorMsg(pub String);
#[derive(Debug)]
pub struct GetJobActorResponse(pub Option<Addr<JobActor>>);

impl Actor for Thingy {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        for j in &self.workpace.jobs {
            let d = self.dir.clone().join(j.name.clone());
            let ja = JobActor::new(j.clone(), d).start();
            self.job_actors.insert(j.name.clone(), ja);
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {}
}

impl Handler<GetJobsMsg> for Thingy {
    type Result = Result<GetJobResponse, std::io::Error>;

    fn handle(&mut self, _msg: GetJobsMsg, _ctx: &mut Self::Context) -> Self::Result {
        Ok(GetJobResponse(self.workpace.jobs.clone()))
    }
}

impl Handler<GetJobActorMsg> for Thingy {
    type Result = Result<GetJobActorResponse, std::io::Error>;

    fn handle(&mut self, msg: GetJobActorMsg, _ctx: &mut Self::Context) -> Self::Result {
        let addr = self
            .job_actors
            .iter()
            .find(|(k, _)| k.eq(&&msg.0))
            .map(|(_, v)| v.clone());
        Ok(GetJobActorResponse(addr))
    }
}
