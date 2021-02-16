use actix::prelude::*;
use root_actor::{Thingy, ThingyMessage, ThingyResponse};
use serde_yaml;
use std::{self, collections::HashSet, path::PathBuf};

use actix_files as fs;
use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};

mod branch_actor;
mod git_utils;
mod job_actor;
mod models;
mod root_actor;

use models::*;

pub type Res = Result<(), Box<dyn std::error::Error>>;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut args = std::env::args();
    if args.len() < 2 {
        eprintln!("Usage: thingy <path to workspace dir containing thingy.yaml>");
        return Ok(());
    }
    args.next();
    let path = args.next();
    if path.is_none() {
        eprintln!("Usage: thingy <path to workspace dir containing thingy.yaml>");
        return Ok(());
    }
    let path = path.unwrap();
    println!("Starting thingy in '{}'", path);

    let p = std::env::current_dir()?;
    let path = p.join(path).canonicalize()?;
    // println!("Changing working directory to '{:?}' ...", path);
    // std::env::set_current_dir(&path)?;
    let ws = validate(&path).unwrap();
    let state = AppState {
        root: Thingy::new(ws, path.into()).start(),
    };

    HttpServer::new(move || {
        App::new()
            .data(state.clone())
            .service(index)
            .service(poll)
            .service(fs::Files::new("/", "./static/").show_files_listing())
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await?;
    println!("shutting down...");
    Ok(())
}

#[derive(Clone)]
struct AppState {
    root: Addr<Thingy>,
}

#[get("/jobs")]
async fn index(data: web::Data<AppState>) -> impl Responder {
    if let Ok(Ok(ThingyResponse::Jobs {jobs})) = data.root.send(ThingyMessage::GetJobs).await {
        return HttpResponse::Ok().json(jobs);
    }
    HttpResponse::InternalServerError().content_type("application/json").body("{\"status\": \"error\"}")
}

#[post("/jobs/{jobId}/poll")]
async fn poll(path: web::Path<(String,)>, data: web::Data<AppState>) -> impl Responder {
    let id = path.into_inner().0;
    if let Ok(Ok(ThingyResponse::Job {addr: Some(addr)})) = data.root.send(ThingyMessage::GetJobActor(id)).await {
        addr.do_send(job_actor::JobMessage::Poll);
    }
    HttpResponse::Ok().content_type("application/json").body("{\"status\": \"OK\"}")
}

fn validate(path: &PathBuf) -> std::result::Result<Workspace, Box<dyn std::error::Error>> {
    println!("Initing thingy in workspace {:?}", &path);

    let ws_yaml_path = path.clone().join("thingy.yaml");

    let md = std::fs::metadata(&ws_yaml_path);
    if let Err(err) = &md {
        return Err(format!(
            "Could not read config from {:?}. Exiting. Does the file exist? Error: {:?}",
            &ws_yaml_path, &err
        )
        .into());
    }
    let md = md.unwrap();
    if !md.is_file() {
        return Err(format!("{:?} is not a regular file. Exiting.", &ws_yaml_path).into());
    }
    let contents = std::fs::read_to_string(&ws_yaml_path);
    if let Err(err) = &contents {
        return Err(format!(
            "Could not read {:?}. Exiting. Error: {:?}",
            &ws_yaml_path, &err
        )
        .into());
    }
    let contents = contents.unwrap();
    let ws = serde_yaml::from_str::<Workspace>(&contents);

    if let Err(err) = &ws {
        return Err(format!(
            "Could not read {:?}. Exiting. Does the file contain valid YAML? Error: {:?}",
            &ws_yaml_path, &err
        )
        .into());
    }

    let mut ws = ws.unwrap();
    let names: Vec<&str> = ws.jobs.iter().map(|j| j.name.trim()).collect();

    let mut uniq = HashSet::<&str>::new();
    for n in names {
        if n.is_empty() {
            return Err("Found job with empty name. Exiting.".into());
        }
        if uniq.contains(n) {
            return Err(format!("Workspace config contains duplicate jobs with name '{}'. Note that names are trimmed when read. Exiting.", n).into());
        }
        uniq.insert(n);
    }

    for j in &mut ws.jobs {
        if let Err(err) = &j.validate() {
            return Err(
                format!("Configuration for {} is invalid: {}. Exiting.", j.name, err).into(),
            );
        }
    }

    // ensure job dirs
    for j in &ws.jobs {
        let name = j.name.trim();
        let dir = path.join(name);

        if dir.is_file() {
            return Err(format!("{:?} is a file. Expected directory or nothing.", &dir).into());
        }

        if !dir.exists() {
            if let Err(err) = std::fs::create_dir_all(&dir) {
                return Err(format!(
                    "Could not create job dir {:?}. Exiting. Error: {:?}",
                    &dir, &err
                )
                .into());
            }
        }
    }

    Ok(ws)
}
