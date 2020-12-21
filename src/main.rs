use chrono::{DateTime, Utc};
use git2::{build::RepoBuilder, Direction, FetchOptions, Repository};
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::{
    self,
    collections::HashSet,
    error::Error,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread::JoinHandle,
    time::Duration,
    time::SystemTime,
};
use tempfile::TempDir;

pub fn main() {
    let mut args = std::env::args();
    if args.len() < 2 {
        eprintln!("Usage: thingy <path to workspace dir containing thingy.yaml>");
        return;
    }
    args.next();
    let path = args.next();
    if path.is_none() {
        eprintln!("Usage: thingy <path to workspace dir containing thingy.yaml>");
        return;
    }
    let path = path.unwrap();
    println!("Starting thingy in '{}'", path);

    match std::env::current_dir() {
        Ok(p) => {
            let path = p.join(path).canonicalize();
            if let Err(err) = &path {
                eprintln!("Could not get canonical dir. Exiting. Error: {:?}", err);
                return;
            }
            let path = path.unwrap();
            println!("Changing working directory to '{:?}' ...", path);
            match std::env::set_current_dir(&path) {
                Ok(_) => {
                    println!("Done.");
                    start(path);
                    return;
                }
                Err(err) => {
                    eprintln!("Could not change current dir. Exiting. Error: {:?}", err);
                    return;
                }
            }
        }
        _ => {
            eprintln!("Could not get current dir. Exiting.");
            return;
        }
    }
}

fn start(path: PathBuf) {
    println!("Initing thingy in workspace {:?}", &path);

    let ws_yaml_path = path.clone().join("thingy.yaml");

    let md = std::fs::metadata(&ws_yaml_path);
    if let Err(err) = &md {
        eprintln!(
            "Could not read config from {:?}. Exiting. Does the file exist? Error: {:?}",
            &ws_yaml_path, &err
        );
        return;
    }
    let md = md.unwrap();
    if !md.is_file() {
        eprintln!("{:?} is not a regular file. Exiting.", &ws_yaml_path);
        return;
    }
    let contents = std::fs::read_to_string(&ws_yaml_path);
    if let Err(err) = &contents {
        eprintln!(
            "Could not read {:?}. Exiting. Error: {:?}",
            &ws_yaml_path, &err
        );
        return;
    }
    let contents = contents.unwrap();
    let ws = serde_yaml::from_str::<Workspace>(&contents);

    if let Err(err) = &ws {
        eprintln!(
            "Could not read {:?}. Exiting. Does the file contain valid YAML? Error: {:?}",
            &ws_yaml_path, &err
        );
        return;
    }

    let ws = ws.unwrap();
    let names: Vec<&str> = ws.jobs.iter().map(|j| j.name.trim()).collect();

    let mut uniq = HashSet::<&str>::new();
    for n in names {
        if n.is_empty() {
            eprintln!("Found job with empty name. Exiting.");
            return;
        }
        if uniq.contains(n) {
            eprintln!("Workspace config contains duplicate jobs with name '{}'. Note that names are trimmed when read. Exiting.", n);
            return;
        }
        uniq.insert(n);
    }

    for j in &ws.jobs {
        if let Err(err) = j.validate() {
            eprintln!("Configuration for {} is invalid: {}. Exiting.", j.name, err);
            return;
        }
    }

    let mut handles: Vec<JoinHandle<()>> = vec![];
    let (s, r) = std::sync::mpsc::channel::<JobEvent>();

    // ensure job dirs
    for j in &ws.jobs {
        let name = j.name.trim();
        let dir = path.join(name);

        if dir.is_file() {
            eprintln!(
                "{:?} is a regular file. Expected directory or nothing.",
                &dir
            );
            return;
        }

        if !dir.exists() {
            if let Err(err) = std::fs::create_dir_all(&dir) {
                eprintln!(
                    "Could not create job dir {:?}. Exiting. Error: {:?}",
                    &dir, &err
                );
                return;
            }
        }

        // start a thread to handle this job
        let job = j.clone();
        let sender = s.clone();
        let t = std::thread::spawn(move || {
            job_work_loop(job, sender, dir);
        });
        handles.push(t);
    }

    while let Ok(je) = r.recv() {
        match &je {
            JobEvent::Tick { job: _ } => {}
            JobEvent::Log {
                job,
                line,
                is_stderr,
            } => {
                let now = SystemTime::now();
                let now: DateTime<Utc> = now.into();
                let now = now.to_rfc3339();

                if *is_stderr {
                    eprintln!("{} [{}] {}", now, job, line);
                } else {
                    println!("{} [{}] {}", now, job, line);
                }
            }
        }
    }
}

fn job_work_loop(job: Job, sender: Sender<JobEvent>, dir: PathBuf) {
    let poll_interval = Duration::from_secs(job.poll_interval_seconds);
    loop {
        let _ = sender.send(JobEvent::Log {
            job: job.name.clone(),
            line: format!("Scanning repo..."),
            is_stderr: true,
        });
        let hash = get_branch_hash(&job.repo_url, &job.branch, job.auth.as_ref());
        match hash {
            Ok(hash) => {
                let hash_file = dir.clone().join("last_commit_hash.txt");
                let old_hash = std::fs::read_to_string(&hash_file).unwrap_or("".into());
                let old_hash = old_hash.trim().to_string();
                if !old_hash.eq(&hash) {
                    // build this commit
                    let clone_dir = dir.clone().join(&hash).join("checkout");
                    if clone_dir.exists() {
                        let _ = std::fs::remove_dir_all(&clone_dir);
                    }
                    if let Err(err) = clone_commit(
                        &job.repo_url,
                        &job.branch,
                        &hash,
                        &clone_dir,
                        job.auth.as_ref(),
                    ) {
                        let _ = sender.send(JobEvent::Log {
                            job: job.name.clone(),
                            line: format!("Could not clone repo. Error: {}", &err),
                            is_stderr: true,
                        });
                    } else {
                        // start the build script in clone dir
                        let cmd = &job.build_script;
                        let mut args: Vec<String> = cmd
                            .split(" ")
                            .filter(|s| !s.is_empty())
                            .map(|s| s.into())
                            .collect();
                        let cmd = args[0].clone();
                        let cmd = clone_dir.clone().join(cmd);
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
                            .current_dir(&clone_dir)
                            .spawn();

                        if let Err(err) = &spawn_result {
                            let je = JobEvent::Log {
                                job: job.name.clone(),
                                line: format!("Could not spawn build process. Error: {}", &err),
                                is_stderr: true,
                            };
                            let _ = sender.send(je);
                            return;
                        }
                        let mut printers: Vec<JoinHandle<()>> = vec![];
                        let mut child = spawn_result.unwrap();
                        let std_out = child.stdout.take();
                        let std_err = child.stderr.take();
                        if let Some(std_out) = std_out {
                            let reader = BufReader::new(std_out);
                            let sender_clone = sender.clone();
                            let name = job.name.clone();
                            let h = std::thread::spawn(move || {
                                reader
                                    .lines()
                                    .filter_map(|line| line.ok())
                                    .for_each(|line| {
                                        let je = JobEvent::Log {
                                            job: name.clone(),
                                            line,
                                            is_stderr: false,
                                        };
                                        let _ = sender_clone.send(je);
                                    });
                            });
                            printers.push(h);
                        }
                        if let Some(std_err) = std_err {
                            let reader = BufReader::new(std_err);
                            let sender_clone = sender.clone();
                            let name = job.name.clone();
                            let h = std::thread::spawn(move || {
                                reader
                                    .lines()
                                    .filter_map(|line| line.ok())
                                    .for_each(|line| {
                                        let je = JobEvent::Log {
                                            job: name.clone(),
                                            line,
                                            is_stderr: true,
                                        };
                                        let _ = sender_clone.send(je);
                                    });
                            });
                            printers.push(h);
                        }

                        for h in printers {
                            let _ = h.join();
                        }

                        let _ = std::fs::write(&hash_file, &hash);
                    }
                }
            }
            Err(err) => {
                let je = JobEvent::Log {
                    job: job.name.clone(),
                    line: format!("Could not get commit hash. Error: {}", &err),
                    is_stderr: true,
                };
                if let Err(err) = sender.send(je) {
                    eprintln!(
                        "[{}] Could not send event. Exiting worker thread. {}",
                        &job.name, &err
                    );
                    return;
                }
            }
        }
        std::thread::sleep(poll_interval);
    }
}

fn clone_commit(
    url: &str,
    branch: &str,
    commit_hash: &str,
    dir: &PathBuf,
    auth: Option<&GitAuth>,
) -> Result<(), Box<dyn Error>> {
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(
        |_user: &str, user_from_url: Option<&str>, _cred: git2::CredentialType| match auth {
            Some(a) => match a {
                GitAuth::PrivateKey { path, passphrase } => git2::Cred::ssh_key(
                    user_from_url.unwrap_or("git"),
                    None,
                    std::path::Path::new(path),
                    passphrase.as_ref().map(|s| s.as_str()),
                ),
            },
            None => git2::Cred::default(),
        },
    );
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(callbacks);

    let repo = RepoBuilder::new()
        .fetch_options(fo)
        .branch(branch)
        .clone(url, dir)?;

    let oid = git2::Oid::from_str(commit_hash)?;
    let commit = repo.find_commit(oid)?;

    repo.branch(commit_hash, &commit, false)?;
    let obj = repo.revparse_single(&("refs/heads/".to_owned() + commit_hash))?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&("refs/heads/".to_owned() + commit_hash))?;

    Ok(())
}

fn get_branch_hash(
    url: &str,
    branch: &str,
    auth: Option<&GitAuth>,
) -> Result<String, Box<dyn Error>> {
    let tmp_dir = TempDir::new()?;
    let repo = Repository::init(tmp_dir.path())?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(
        |_user: &str, user_from_url: Option<&str>, _cred: git2::CredentialType| match auth {
            Some(a) => match a {
                GitAuth::PrivateKey { path, passphrase } => git2::Cred::ssh_key(
                    user_from_url.unwrap_or("git"),
                    None,
                    std::path::Path::new(path),
                    passphrase.as_ref().map(|s| s.as_str()),
                ),
            },
            None => git2::Cred::default(),
        },
    );

    let mut remote = repo.remote("origin", url)?;
    let connection = remote.connect_auth(Direction::Fetch, Some(callbacks), None)?;
    let l = connection.list()?.iter().find(|head| {
        let rf = head.name();
        rf == format!("refs/heads/{}", branch)
    });
    match l {
        Some(rf) => Ok(rf.oid().to_string()),
        _ => Err("Could not find branch".into()),
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum GitAuth {
    PrivateKey {
        path: String,
        passphrase: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum JobEvent {
    Tick {
        job: String,
    },
    Log {
        job: String,
        line: String,
        is_stderr: bool,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Workspace {
    jobs: Vec<Job>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Job {
    name: String,
    repo_url: String,
    branch: String,
    build_script: String,
    poll_interval_seconds: u64,
    auth: Option<GitAuth>,
}

impl Job {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
