#![forbid(unsafe_code)]
use std::{collections::HashMap, fmt::Display, path::PathBuf};

use actix::prelude::*;
use branch_actor::{BuildNowMsg, GetBranchDetailsMsg, GetBuildActorMsg, GetBuildLogLinesMsg};
use build_actor::StopBuildMessage;
use job_actor::{GetBranchActorMsg, GetJobDetailsMsg, JobPollMsg};
use thingy::{
    AddJobMsg, GetJobActorMsg, GetJobActorResponse, GetJobsMsg, RemoveJobMsg, Thingy,
};

use actix_files as fs;
use actix_web::{
    delete,
    dev::HttpResponseBuilder,
    get,
    http::{header, StatusCode},
    post, web, App, HttpResponse, HttpServer, Responder,
};

use serde::Deserialize;

mod branch_actor;
mod build_actor;
mod git_utils;
mod job_actor;
mod models;
mod thingy;

use models::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut args = std::env::args();
    if args.len() < 2 {
        eprintln!("Usage: thingy <path to workspace dir>");
        return Ok(());
    }
    args.next();
    let path = args.next();
    if path.is_none() {
        eprintln!("Usage: thingy <path to workspace dir>");
        return Ok(());
    }
    let path = path.unwrap();

    let path = PathBuf::from(path).canonicalize()?;
    let ws = Workspace::from_dir_path(&path).unwrap();
    let state = ThingyState {
        root: Thingy::new(ws, path.into()).start(),
    };

    let listen_addr: String = if let Ok(addr) = std::env::var("LISTEN_ADDRESS") {
        addr
    } else {
        "127.0.0.1".into()
    };
    let port: u16 = if let Ok(Ok(p)) = std::env::var("LISTEN_PORT").map(|pstr| pstr.parse()) {
        p
    } else {
        8080
    };
    HttpServer::new(move || {
        let mut app = App::new()
            .data(state.clone())
            .service(index)
            .service(get_jobs)
            .service(create_job)
            .service(delete_job)
            .service(poll)
            .service(get_job)
            .service(get_branch)
            .service(force_build)
            .service(get_build_log)
            .service(abort_build);
        if let Ok(_) = std::env::var("SERVE_STATIC") {
            app = app.service(fs::Files::new("/", "./static/").show_files_listing());
        }
        app
    })
    .bind((listen_addr, port))?
    .run()
    .await?;
    println!("shutting down...");
    Ok(())
}

/// Process-wide state
#[derive(Clone)]
struct ThingyState {
    /// address of the root actor
    root: Addr<Thingy>,
}

/// Contents of static/index.html , served at GET /
static HTML_BYTES: &[u8] = include_bytes!("../static/index.html");

/// Struct for error messages
#[derive(Debug)]
struct ApiMessage {
    status: StatusCode,
    message: String,
}

/// Converts to JSON string
impl Display for ApiMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut response: HashMap<String, String> = HashMap::new();
        response.insert("message".into(), self.message.clone());
        f.write_str(&serde_json::to_string(&response).unwrap())
    }
}

impl ApiMessage {
    fn new() -> Self {
        ApiMessage {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal server error".into(),
        }
    }
    fn new_with_status(status: StatusCode, message: &str) -> Self {
        ApiMessage {
            status,
            message: message.into(),
        }
    }
}

/// Convert ApiMessage to an actix error
impl actix_web::error::ResponseError for ApiMessage {
    fn status_code(&self) -> actix_web::http::StatusCode {
        self.status
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponseBuilder::new(self.status_code())
            .set_header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(self.to_string())
    }
}

/// Convert actor error to ApiMessage
impl From<MailboxError> for ApiMessage {
    fn from(_: MailboxError) -> Self {
        ApiMessage::new()
    }
}

/// Convert I/O error to ApiMessage
impl From<std::io::Error> for ApiMessage {
    // TODO: get description from io error
    fn from(_: std::io::Error) -> Self {
        ApiMessage::new()
    }
}

/// Index page, serves contents of static/index.html
/// index.html contains all the ui code for thingy
#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(HTML_BYTES)
}

/// List jobs
#[get("/jobs")]
async fn get_jobs(data: web::Data<ThingyState>) -> Result<HttpResponse, ApiMessage> {
    Ok(HttpResponse::Ok().json(data.root.send(GetJobsMsg).await??.0))
}

/// Add new job to workspace, this updates the <workspace>/thingy.yaml file
#[post("/jobs")]
async fn create_job(
    req: web::Json<Job>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    data.root.send(AddJobMsg(req.into_inner())).await??;
    Ok(HttpResponse::NoContent().body(""))
}

/// Remove a job from workspace, this updates the <workspace>/thingy.yaml file
/// Any ongoing builds related to thsi job will not be stopped immediately
#[delete("/jobs/{jobId}")]
async fn delete_job(
    path: web::Path<(String,)>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let id = path.into_inner().0;
    data.root.send(RemoveJobMsg(id)).await??;
    Ok(HttpResponse::NoContent().body(""))
}

/// Poll a job's repository URL now. This overrides any poll interval set
/// or the job, and resets it so that next automatic poll will happen after
// the usual duration of this command
#[post("/jobs/{jobId}/poll")]
async fn poll(
    path: web::Path<(String,)>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let id = path.into_inner().0;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(id)).await?? {
        addr.do_send(JobPollMsg);
        Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body("{\"status\": \"OK\"}"))
    } else {
        Err(ApiMessage::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

/// Get details of a job
#[get("/jobs/{jobId}")]
async fn get_job(
    path: web::Path<(String,)>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let id = path.into_inner().0;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(id)).await?? {
        Ok(HttpResponse::Ok().json(addr.send(GetJobDetailsMsg).await??))
    } else {
        Err(ApiMessage::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[get("/jobs/{jobId}/branches/{branch}")]
async fn get_branch(
    path: web::Path<(String, String)>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let path = path.into_inner();
    let job_id = path.0;
    let branch = path.1;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(job_id)).await?? {
        if let Some(addr) = addr.send(GetBranchActorMsg(branch)).await?? {
            Ok(HttpResponse::Ok().json(addr.send(GetBranchDetailsMsg).await??))
        } else {
            Err(ApiMessage::new_with_status(
                StatusCode::NOT_FOUND,
                "Not found",
            ))
        }
    } else {
        Err(ApiMessage::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[derive(Deserialize)]
struct LogRequest {
    start: u32,
    num_lines: u32,
}

#[get("/jobs/{jobId}/branches/{branch}/builds/{build_num}/log")]
async fn get_build_log(
    path: web::Path<(String, String, u64)>,
    req_info: web::Query<LogRequest>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let path = path.into_inner();
    let job_id = path.0;
    let branch = path.1;
    let build_num = path.2;
    let info = req_info.into_inner();
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(job_id)).await?? {
        if let Some(addr) = addr.send(GetBranchActorMsg(branch)).await?? {
            Ok(HttpResponse::Ok().json(
                addr.send(GetBuildLogLinesMsg {
                    build_num,
                    start: info.start,
                    num_lines: info.num_lines,
                })
                .await??,
            ))
        } else {
            Err(ApiMessage::new_with_status(
                StatusCode::NOT_FOUND,
                "Not found",
            ))
        }
    } else {
        Err(ApiMessage::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[post("/jobs/{jobId}/branches/{branch}/builds")]
async fn force_build(
    path: web::Path<(String, String)>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let path = path.into_inner();
    let job_id = path.0;
    let branch = path.1;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(job_id)).await?? {
        if let Some(addr) = addr.send(GetBranchActorMsg(branch)).await?? {
            addr.do_send(BuildNowMsg);
            return Err(ApiMessage::new_with_status(StatusCode::OK, "OK"));
        }
    }
    Err(ApiMessage::new_with_status(
        StatusCode::NOT_FOUND,
        "Not found",
    ))
}

#[delete("/jobs/{jobId}/branches/{branch}/builds/{build_num}")]
async fn abort_build(
    path: web::Path<(String, String, u64)>,
    data: web::Data<ThingyState>,
) -> Result<HttpResponse, ApiMessage> {
    let path = path.into_inner();
    let job_id = path.0;
    let branch = path.1;
    let build_num = path.2;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(job_id)).await?? {
        if let Some(addr) = addr.send(GetBranchActorMsg(branch)).await?? {
            if let Some(addr) = addr.send(GetBuildActorMsg(build_num)).await?? {
                addr.do_send(StopBuildMessage);
                return Err(ApiMessage::new_with_status(StatusCode::OK, "OK"));
            }
        }
    }
    Err(ApiMessage::new_with_status(
        StatusCode::NOT_FOUND,
        "Not found",
    ))
}
