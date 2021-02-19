use actix::prelude::*;
use std::io::prelude::*;
use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    writeln,
};

use crate::branch_actor::{BranchActor, BuildStoppedMsg};

#[derive(Debug)]
pub struct BuildActor {
    command: String,
    dir: PathBuf,
    commit_hash: Option<String>,
    parent: Addr<BranchActor>,
    log_file_path: PathBuf,
    process: Option<Child>,
    num: u64,
    status: String,
}

impl BuildActor {
    pub fn new(
        command: String,
        dir: PathBuf,
        commit_hash: Option<String>,
        parent: Addr<BranchActor>,
        log_file_path: PathBuf,
        num: u64,
    ) -> Self {
        BuildActor {
            command,
            dir,
            commit_hash,
            parent,
            log_file_path,
            process: None,
            num,
            status: "finished".into(),
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
            // self.process.replace(child);

            // spawn threada to transfer buffers and notify actor
            let reader = BufReader::new(std_out);
            let log_file = self.log_file_path.clone();
            let h = std::thread::spawn(move || {
                let mut file = OpenOptions::new()
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(log_file)
                    .unwrap();

                reader
                    .lines()
                    .filter_map(|line| line.ok())
                    .for_each(|line| {
                        let _ = writeln!(file, "[out] {}", line);
                    });
            });
            let reader = BufReader::new(std_err);
            let log_file = self.log_file_path.clone();
            let h2 = std::thread::spawn(move || {
                let mut file = OpenOptions::new()
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(log_file)
                    .unwrap();
                reader
                    .lines()
                    .filter_map(|line| line.ok())
                    .for_each(|line| {
                        let _ = writeln!(file, "[err] {}", line);
                    });
            });
            let adr = _ctx.address();
            let _ = std::thread::spawn(move || {
                let _ = h.join();
                let _ = h2.join();
                adr.do_send(StopBuildMessage);
            });
        } else {
            _ctx.stop();
        }
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        match self.process.take() {
            Some(mut ch) => {
                if let Ok(Some(status)) = ch.try_wait() {
                    self.status = if status.success() {
                        "finished".into()
                    } else {
                        "error".into()
                    };
                } else {
                    let _ = ch.kill();
                    if let Ok(Some(status)) = ch.try_wait() {
                        self.status = if status.success() {
                            "finished".into()
                        } else {
                            "error".into()
                        };
                    }
                }
            }
            None => {}
        }
        self.parent.do_send(BuildStoppedMsg {
            build_num: self.num,
            status: self.status.clone(),
        });
        println!("Build finished");
    }
}

#[derive(Message, Debug)]
#[rtype(result = "Result<(), std::io::Error>")]
pub struct StopBuildMessage;

impl Handler<StopBuildMessage> for BuildActor {
    type Result = Result<(), std::io::Error>;

    fn handle(&mut self, _msg: StopBuildMessage, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(_ctx.stop())
    }
}
