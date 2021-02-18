use actix::prelude::*;
use branch_actor::BranchResponse;
use job_actor::JobResponse;
use root_actor::{Thingy, ThingyMessage, ThingyResponse};

use actix_files as fs;
use actix_web::{delete, get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};

mod branch_actor;
mod build_actor;
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
    let ws = Workspace::from_dir_path(&path).unwrap();
    let state = AppState {
        root: Thingy::new(ws, path.into()).start(),
    };

    HttpServer::new(move || {
        App::new()
            .data(state.clone())
            .service(index)
            .service(poll)
            .service(get_job)
            .service(get_branch)
            .service(force_build)
            .service(get_build_log)
            .service(abort_build)
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
    if let Ok(Ok(ThingyResponse::Jobs { jobs })) = data.root.send(ThingyMessage::GetJobs).await {
        HttpResponse::Ok().json(jobs)
    } else {
        HttpResponse::InternalServerError()
            .content_type("application/json")
            .body("{\"status\": \"error\"}")
    }
}

#[post("/jobs/{jobId}/poll")]
async fn poll(path: web::Path<(String,)>, data: web::Data<AppState>) -> impl Responder {
    let id = path.into_inner().0;
    if let Ok(Ok(ThingyResponse::Job { addr: Some(addr) })) =
        data.root.send(ThingyMessage::GetJobActor(id)).await
    {
        addr.do_send(job_actor::JobMessage::Poll);
        HttpResponse::Ok()
            .content_type("application/json")
            .body("{\"status\": \"OK\"}")
    } else {
        HttpResponse::NotFound().body("")
    }
}

#[get("/jobs/{jobId}")]
async fn get_job(path: web::Path<(String,)>, data: web::Data<AppState>) -> impl Responder {
    let id = path.into_inner().0;
    if let Ok(Ok(ThingyResponse::Job { addr: Some(addr) })) =
        data.root.send(ThingyMessage::GetJobActor(id)).await
    {
        if let Ok(Ok(JobResponse::JobDetails(res))) = addr.send(job_actor::JobMessage::GetDetails).await {
            HttpResponse::Ok().json(res)
        } else {
            HttpResponse::NotFound().body("")
        }
    } else {
        HttpResponse::NotFound().body("")
    }
}

#[get("/jobs/{jobId}/branches/{branch}")]
async fn get_branch(path: web::Path<(String,String)>, data: web::Data<AppState>) -> impl Responder {
    let path = path.into_inner();
    let id = path.0;
    if let Ok(Ok(ThingyResponse::Job { addr: Some(addr) })) =
        data.root.send(ThingyMessage::GetJobActor(id)).await
    {
        if let Ok(Ok(JobResponse::Branch {addr: Some(ba)})) = addr.send(job_actor::JobMessage::GetBranchActor(path.1)).await {
            // get details from branch
            if let Ok(Ok(BranchResponse::Details(details))) = ba.send(branch_actor::BranchMessage::GetDetails).await {
                return HttpResponse::Ok().json(details);
            }
            HttpResponse::NotFound().body("")
        } else {
            HttpResponse::NotFound().body("")
        }
    } else {
        HttpResponse::NotFound().body("")
    }
}

#[post("/jobs/{jobId}/branches/{branch}/builds")]
async fn force_build(req: HttpRequest) -> impl Responder {
    println!("{}", req.path());
    ""
}

#[get("/jobs/{jobId}/branches/{branch}/builds/{hash}/log")]
async fn get_build_log(req: HttpRequest) -> impl Responder {
    println!("{}", req.path());
    ""
}

#[delete("/jobs/{jobId}/branches/{branch}/builds/{hash}")]
async fn abort_build(req: HttpRequest) -> impl Responder {
    println!("{}", req.path());
    ""
}
