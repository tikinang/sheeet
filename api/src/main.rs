use actix_cors::Cors;
use actix_files::Files;
use actix_web::{post, put, web, App, Error, HttpResponse, HttpServer};
// TODO: Can be replaced with std lib?
use bytes::Bytes;
use futures_util::stream::{self, StreamExt};
use futures_util::Stream;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
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

    fn stream_command(&self, command: &mut Command) -> Result<std::process::Child, Error> {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                actix_web::error::ErrorInternalServerError(format!(
                    "failed to spawn command: {err}",
                ))
            })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            actix_web::error::ErrorInternalServerError("failed to capture stdout")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            actix_web::error::ErrorInternalServerError("failed to capture stderr")
        })?;

        self.spawn_buff_line_reading(stdout, StreamEvent::StdoutLine);
        self.spawn_buff_line_reading(stderr, StreamEvent::StderrLine);

        Ok(child)
    }

    fn spawn_buff_line_reading(
        &self,
        pipe: impl Read + Send + 'static,
        line_constructor: fn(String) -> StreamEvent,
    ) {
        let sender = self.new_sender();
        // TODO: Shouldn't this be async?
        thread::spawn(move || {
            let reader = BufReader::new(pipe);
            for line in reader.lines() {
                if let Ok(line) = line {
                    let _ = sender.send(line_constructor(line.trim().into()));
                }
            }
        });
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
    let streaming_responder = StreamingResponder::new();

    let path = format!(
        "{}/{}",
        env::var(ENV_KEY_API_WORKSPACES_PATH).unwrap_or(DEFAULT_WORKSPACES_PATH.into()),
        &query.workspace_id
    );

    if !fs::exists(&path)? {
        // TODO: This is wrong and temporary. Users should not be allowed to use their own workspace IDs.
        initialize_workspace(&query.workspace_id)?;
    }

    fs::write(format!("{path}/src/lib.rs"), body.lib_rs.as_str())?;
    fs::write(format!("{path}/Cargo.toml"), body.cargo_toml.as_str())?;
    fs::write(format!("{path}/index.html"), include_str!("user.html"))?;

    let mut child = streaming_responder.stream_command(
        Command::new("trunk")
            .arg("build")
            .current_dir(&path)
            .env("RUST_LOG", "info"),
    )?;

    let finish_sender = streaming_responder.new_sender();
    thread::spawn(move || match child.wait() {
        Ok(status) if status.success() => {
            _ = finish_sender.send(StreamEvent::DownloadInfo(DownloadInfo {
                js_download_url: format!("/workspaces/{}/dist/sheeet-lib.js", query.workspace_id),
                wasm_download_url: format!(
                    "/workspaces/{}/dist/sheeet-lib_bg.wasm",
                    query.workspace_id
                ),
            }));
            println!("Build completed successfully");
        }
        Ok(status) => {
            _ = finish_sender.send(StreamEvent::Error(format!("build failed: {status}")));
            println!("Build failed");
        }
        Err(err) => {
            _ = finish_sender.send(StreamEvent::Error(format!("build failed with err: {err}")));
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
