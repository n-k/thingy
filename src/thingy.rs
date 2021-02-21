use std::{
    collections::HashMap,
    fs::create_dir_all,
    io::{Error, ErrorKind},
    path::PathBuf,
};

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

    pub fn sync_ws_to_disk(&self) -> Result<(), std::io::Error> {
        let file_path = self.dir.join("thingy.yaml");
        let yaml = serde_yaml::to_string(&self.workpace)
            .map_err(|_e| Error::new(ErrorKind::Other, "Could not write yaml"))?;
        std::fs::write(&file_path, yaml)?;

        Ok(())
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

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct AddJobMsg(pub Job);

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct RemoveJobMsg(pub String);

impl Actor for Thingy {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        for j in &self.workpace.jobs {
            let d = self.dir.join(j.name.clone());
            let ja = JobActor::new(j.clone(), d).start();
            self.job_actors.insert(j.name.clone(), ja);
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {}
}

impl Handler<GetJobsMsg> for Thingy {
    type Result = Result<GetJobResponse, Error>;

    fn handle(&mut self, _msg: GetJobsMsg, _ctx: &mut Self::Context) -> Self::Result {
        Ok(GetJobResponse(self.workpace.jobs.clone()))
    }
}

impl Handler<GetJobActorMsg> for Thingy {
    type Result = Result<GetJobActorResponse, Error>;

    fn handle(&mut self, msg: GetJobActorMsg, _ctx: &mut Self::Context) -> Self::Result {
        let addr = self
            .job_actors
            .iter()
            .find(|(k, _)| k.eq(&&msg.0))
            .map(|(_, v)| v.clone());
        Ok(GetJobActorResponse(addr))
    }
}

impl Handler<AddJobMsg> for Thingy {
    type Result = Result<(), Error>;

    fn handle(&mut self, msg: AddJobMsg, _ctx: &mut Self::Context) -> Self::Result {
        let mut job = msg.0;
        if let Err(s) = job.validate() {
            return Err(Error::new(ErrorKind::Other, s.as_str()));
        }
        if self
            .workpace
            .jobs
            .iter()
            .find(|j| j.name.eq(&job.name))
            .is_some()
        {
            return Err(Error::new(
                ErrorKind::Other,
                "Job with this name already exists",
            ));
        }
        self.workpace.jobs.push(job.clone());

        let d = self.dir.join(job.name.clone());
        create_dir_all(&d)?;
        let ja = JobActor::new(job.clone(), d).start();
        self.job_actors.insert(job.name.clone(), ja);

        self.sync_ws_to_disk()
    }
}

impl Handler<RemoveJobMsg> for Thingy {
    type Result = Result<(), Error>;

    fn handle(&mut self, _msg: RemoveJobMsg, _ctx: &mut Self::Context) -> Self::Result {
        // Remove the job actor's address from this actor. This is the only place to hold job actor's address,
        // so removing it will stop the job actor.
        self.job_actors = self
            .job_actors
            .clone()
            .into_iter()
            .filter(|ja| !ja.0.eq(&_msg.0))
            .collect();
        self.workpace.jobs = self
            .workpace
            .jobs
            .clone()
            .into_iter()
            .filter(|j| !j.name.eq(&_msg.0))
            .collect();

        self.sync_ws_to_disk()
    }
}
