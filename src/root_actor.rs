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
#[rtype(result = "Result<ThingyResponse, std::io::Error>")]
pub enum ThingyMessage {
    GetJobs,
    GetJobActor(String),
}

#[derive(Debug)]
pub enum ThingyResponse {
    Jobs { jobs: Vec<Job> },
    Job { addr: Option<Addr<JobActor>> },
}

impl Actor for Thingy {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        for j in &self.workpace.jobs {
            let d = self.dir.clone().join(j.name.clone());
            let ja = JobActor::new(j.clone(), d).start();
            self.job_actors.insert(j.name.clone(), ja);
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
    }
}

impl Handler<ThingyMessage> for Thingy {
    type Result = Result<ThingyResponse, std::io::Error>;

    fn handle(&mut self, _msg: ThingyMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match _msg {
            ThingyMessage::GetJobs => {
                return Ok(ThingyResponse::Jobs {
                    jobs: self.workpace.jobs.clone(),
                });
            }
            ThingyMessage::GetJobActor(name) => {
                let addr = self
                    .job_actors
                    .iter()
                    .find(|(k, _)| k.eq(&&name))
                    .map(|(_, v)| v.clone());
                Ok(ThingyResponse::Job { addr })
            }
        }
    }
}
