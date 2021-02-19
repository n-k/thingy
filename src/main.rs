use std::{fmt::Display, path::PathBuf};

use actix::prelude::*;
use branch_actor::{GetBranchDetailsMsg, GetBuildLogLinesMsg};
use job_actor::{GetBranchActorMsg, GetJobDetailsMsg, JobPollMsg};
use root_actor::{GetJobActorMsg, GetJobActorResponse, GetJobsMsg, Thingy};

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

    let path = PathBuf::from(path).canonicalize()?;
    let ws = Workspace::from_dir_path(&path).unwrap();
    let state = AppState {
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

#[derive(Clone)]
struct AppState {
    root: Addr<Thingy>,
}

static HTML_BYTES: &[u8] = include_bytes!("../static/index.html");

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!(r#"{{"message": "{}"}}"#, self.message).as_str())
    }
}

impl ApiError {
    fn new() -> Self {
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal server error".into(),
        }
    }
    fn new_with_status(status: StatusCode, message: &str) -> Self {
        ApiError {
            status,
            message: message.into(),
        }
    }
}

impl actix_web::error::ResponseError for ApiError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        self.status
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponseBuilder::new(self.status_code())
            .set_header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(self.to_string())
    }
}

impl From<MailboxError> for ApiError {
    fn from(_: MailboxError) -> Self {
        ApiError::new()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(_: std::io::Error) -> Self {
        ApiError::new()
    }
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(HTML_BYTES)
}

#[get("/jobs")]
async fn get_jobs(data: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Ok().json(data.root.send(GetJobsMsg).await??.0))
}

#[post("/jobs/{jobId}/poll")]
async fn poll(
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner().0;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(id)).await?? {
        addr.do_send(JobPollMsg);
        Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body("{\"status\": \"OK\"}"))
    } else {
        Err(ApiError::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[get("/jobs/{jobId}")]
async fn get_job(
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner().0;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(id)).await?? {
        Ok(HttpResponse::Ok().json(addr.send(GetJobDetailsMsg).await??))
    } else {
        Err(ApiError::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[get("/jobs/{jobId}/branches/{branch}")]
async fn get_branch(
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let path = path.into_inner();
    let job_id = path.0;
    let branch = path.1;
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(job_id)).await?? {
        if let Some(addr) = addr.send(GetBranchActorMsg(branch)).await?? {
            Ok(HttpResponse::Ok().json(addr.send(GetBranchDetailsMsg).await??))
        } else {
            Err(ApiError::new_with_status(
                StatusCode::NOT_FOUND,
                "Not found",
            ))
        }
    } else {
        Err(ApiError::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[derive(Deserialize)]
struct LogRequestInfo {
    start: u32,
    num_lines: u32,
}

#[get("/jobs/{jobId}/branches/{branch}/builds/{build_num}/log")]
async fn get_build_log(
    path: web::Path<(String, String, u64)>,
    req_info: web::Query<LogRequestInfo>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, ApiError> {
    let path = path.into_inner();
    let job_id = path.0;
    let branch = path.1;
    let build_num = path.2;
    let info = req_info.into_inner();
    if let GetJobActorResponse(Some(addr)) = data.root.send(GetJobActorMsg(job_id)).await?? {
        if let Some(addr) = addr.send(GetBranchActorMsg(branch)).await?? {
            Ok(HttpResponse::Ok().json(addr.send(GetBuildLogLinesMsg { build_num, start: info.start, num_lines: info.num_lines }).await??))
        } else {
            Err(ApiError::new_with_status(
                StatusCode::NOT_FOUND,
                "Not found",
            ))
        }
    } else {
        Err(ApiError::new_with_status(
            StatusCode::NOT_FOUND,
            "Not found",
        ))
    }
}

#[post("/jobs/{jobId}/branches/{branch}/builds")]
async fn force_build() -> Result<HttpResponse, ApiError> {
    Err(ApiError::new())
}

#[delete("/jobs/{jobId}/branches/{branch}/builds/{hash}")]
async fn abort_build() -> Result<HttpResponse, ApiError> {
    Err(ApiError::new())
}
