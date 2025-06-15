use actix_cors::Cors;
use actix_files::Files;
use actix_web::{post, put, web, App, Error, HttpResponse, HttpServer};
// TODO: Can be replaced with std lib?
use bytes::Bytes;
use futures_util::stream::{self, StreamExt};
// TODO: Can be replaced with std lib?
use futures_util::Stream;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::{env, fs, thread};

#[derive(Deserialize)]
struct CompileQuery {
    workspace_id: String,
}

#[derive(Deserialize)]
struct CompileBody {
    lib_rs: String,
    cargo_toml: String,
}

#[derive(Serialize)]
struct DownloadInfo {
    js_download_url: String,
    wasm_download_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum StreamEvent {
    StdoutLine(String),
    StderrLine(String),
    Error(String),
    DownloadInfo(DownloadInfo),
}

struct StreamingResponder {
    receiver: Receiver<StreamEvent>,
    sender: Sender<StreamEvent>,
}

impl StreamingResponder {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        StreamingResponder { receiver, sender }
    }

    fn new_sender(&self) -> Sender<StreamEvent> {
        self.sender.clone()
    }

    /// Consumes the responder and produces stream of all messages that receiver will receive.
    fn produce_stream(self) -> impl Stream<Item = Result<Bytes, Error>> + 'static {
        stream::unfold(self.receiver, |receiver| async move {
            if let Ok(line) = receiver.recv() {
                return Some((Ok(line), receiver));
            }
            None // Channel closed.
        })
        .map(|result: Result<_, Error>| match result {
            Ok(event) => Ok(Bytes::from(format!(
                "data: {}\n",
                serde_json::to_string(&event)?
            ))),
            Err(err) => Ok(Bytes::from(format!(
                "data: {}\n\n",
                serde_json::to_string(&StreamEvent::Error(err.to_string()))?
            ))),
        })
    }
}

#[put("/compile")]
async fn compile(
    body: web::Json<CompileBody>,
    query: web::Query<CompileQuery>,
) -> Result<HttpResponse, Error> {
    // TODO: Error handling.

    println!("workspace ID: {}", &query.workspace_id);
    let path = format!(
        "{}/{}",
        env::var(ENV_KEY_API_WORKSPACES_PATH).unwrap_or(DEFAULT_WORKSPACES_PATH.into()),
        &query.workspace_id
    );

    if !fs::exists(&path)? {
        // TODO: This is wrong and temporary. Users should not be allowed to use their own workspace IDs.
        initialize_workspace(&query.workspace_id)?;
    }

    fs::write(format!("{path}/src/lib.rs"), body.lib_rs.as_str()).unwrap();
    fs::write(format!("{path}/Cargo.toml"), body.cargo_toml.as_str()).unwrap();
    fs::write(format!("{path}/index.html"), include_str!("user.html")).unwrap();

    println!("workspace PATH: {}", path);
    let mut child = Command::new("trunk")
        .current_dir(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("build")
        .spawn()
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("failed to start trunk build: {e}",))
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("failed to capture trunk build stdout")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("failed to capture trunk build stderr")
    })?;

    let streaming_responder = StreamingResponder::new();

    let stdout_sender = streaming_responder.new_sender();
    let stderr_sender = streaming_responder.new_sender();
    let finished_sender = streaming_responder.new_sender();

    // TODO: Shouldn't these be async?
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                let _ = stdout_sender.send(StreamEvent::StdoutLine(line.trim().into()));
            }
        }
    });
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                let _ = stderr_sender.send(StreamEvent::StderrLine(line.trim().into()));
            }
        }
    });

    thread::spawn(move || match child.wait() {
        Ok(status) if status.success() => {
            _ = finished_sender.send(StreamEvent::DownloadInfo(DownloadInfo {
                js_download_url: format!("/workspaces/{}/dist/sheeet-lib.js", query.workspace_id),
                wasm_download_url: format!(
                    "/workspaces/{}/dist/sheeet-lib_bg.wasm",
                    query.workspace_id
                ),
            }));
            println!("Build completed successfully");
        }
        Ok(status) => {
            _ = finished_sender.send(StreamEvent::Error(format!("build failed: {status}")));
            println!("Build failed");
        }
        Err(err) => {
            _ = finished_sender.send(StreamEvent::Error(format!("build failed with err: {err}")));
            println!("Build failed with err");
        }
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(streaming_responder.produce_stream()))
}

#[derive(Serialize)]
struct InitializeResponse {
    workspace_id: String,
}

#[post("/initialize")]
async fn initialize() -> Result<HttpResponse, Error> {
    let workspace_id: String = rand::rng()
        .sample_iter(&rand::distr::Alphabetic)
        .take(12)
        .map(char::from)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    println!("{}", workspace_id);

    initialize_workspace(&workspace_id)?;

    Ok(HttpResponse::Ok().json(InitializeResponse { workspace_id }))
}

fn initialize_workspace(workspace_id: &str) -> Result<(), Error> {
    let path = format! {"{}/{}", env::var(ENV_KEY_API_WORKSPACES_PATH).unwrap_or(DEFAULT_WORKSPACES_PATH.into()), &workspace_id};

    match fs::create_dir_all(&path) {
        Err(error) => {
            return Err(actix_web::error::ErrorInternalServerError(format!(
                "create dir all '{path}': {}",
                error.to_string()
            )));
        }
        _ => (),
    };

    match Command::new("cargo")
        .current_dir(&path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg("init")
        .arg("--lib")
        .arg("--name")
        .arg("sheeet-lib")
        .status()
    {
        Err(error) => {
            return Err(actix_web::error::ErrorInternalServerError(format!(
                "run cargo init: '{path}': {}",
                error.to_string()
            )));
        }
        Ok(status) => {
            if !status.success() {
                return Err(actix_web::error::ErrorInternalServerError(format!(
                    "run cargo init: exit status: {:?}",
                    status
                )));
            }
        }
    };

    Ok(())
}

// TODO: Improve configurability.
const DEFAULT_WORKSPACES_PATH: &str = "/home/tikinang/workspaces";
const ENV_KEY_API_WORKSPACES_PATH: &str = "WORKSPACES_PATH";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    HttpServer::new(|| {
        // TODO: CORS.
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();

        App::new()
            .service(Files::new(
                "/workspaces",
                env::var(ENV_KEY_API_WORKSPACES_PATH).unwrap_or(DEFAULT_WORKSPACES_PATH.into()),
            ))
            .wrap(cors)
            .service(compile)
            .service(initialize)
    })
    .bind(("0.0.0.0", 8080))?
    .workers(4)
    .run()
    .await
}
