use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use actix::prelude::*;

use crate::branch_actor::BranchActor;

#[derive(Debug)]
pub struct BuildActor {
    command: String,
    dir: PathBuf,
    commit_hash: Option<String>,
    parent: Addr<BranchActor>,
    process: Option<Child>,
}

impl BuildActor {
    pub fn new(
        command: String,
        dir: PathBuf,
        commit_hash: Option<String>,
        parent: Addr<BranchActor>,
    ) -> Self {
        BuildActor {
            command,
            dir,
            commit_hash,
            parent,
            process: None,
        }
    }
}

impl Actor for BuildActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        println!("Build started");
        let mut args: Vec<String> = self
            .command
            .as_str()
            .split(" ")
            .filter(|s| !s.is_empty())
            .map(|s| s.into())
            .collect();
        let cmd = args[0].clone();
        let cmd = self.dir.join(cmd);
        args.drain(0..1);

        let mut command = Command::new(cmd);
        command.args(args);
        let spawn_result = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // always adding this, in case the child process has something
            // to do with python and has the insane behavior of not flushing
            // std stream file deccriptors on print
            .env("PYTHONUNBUFFERED", "1")
            // .env("BRANCH", &self.branch)
            .current_dir(&self.dir)
            .spawn();
        if let Ok(mut child) = spawn_result {
            let std_out = child.stdout.take().unwrap();
            let std_err = child.stderr.take().unwrap();
            self.process.replace(child);

            // spawn threada to transfer buffers and notify actor
            let reader = BufReader::new(std_out);
            let h = std::thread::spawn(move || {
                reader
                    .lines()
                    .filter_map(|line| line.ok())
                    .for_each(|line| {
                        println!("{}", line);
                    });
            });
            let reader = BufReader::new(std_err);
            let h2 = std::thread::spawn(move || {
                reader
                    .lines()
                    .filter_map(|line| line.ok())
                    .for_each(|line| {
                        eprintln!("{}", line);
                    });
            });
            let adr = _ctx.address();
            let _ = std::thread::spawn(move || {
                let _ = h.join();
                let _ = h2.join();
                adr.do_send(BuildMessage::Stop);
            })
            .join();
        } else {
            _ctx.stop();
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        match self.process.take() {
            Some(mut ch) => {
                let _ = ch.kill();
            }
            None => {}
        }
        self.parent
            .do_send(crate::branch_actor::BranchMessage::BuildStopped {
                addr: _ctx.address(),
            });
        println!("Build stopped");
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub enum BuildMessage {
    Stop,
}

impl Handler<BuildMessage> for BuildActor {
    type Result = Result<(), std::io::Error>;

    fn handle(&mut self, _msg: BuildMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match _msg {
            BuildMessage::Stop => {
                _ctx.stop();
            }
        }
        Ok(())
    }
}
