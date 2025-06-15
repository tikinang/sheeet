use actix_cors::Cors;
use actix_files::Files;
// TODO: Can be replaced with std lib?
use actix_web::{post, put, web, App, Error, HttpResponse, HttpServer};
use bytes::Bytes;
use futures_util::stream::{self, StreamExt};
use futures_util::Stream;
use log::info;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::{env, fs, thread};

#[derive(Deserialize)]
struct CompileQuery {
    workspace_id: Option<String>,
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
    workspace_id: String,
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

    fn log_line(&self, line: String, line_constructor: fn(String) -> StreamEvent) {
        _ = self.sender.send(line_constructor(line));
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
                "data: {}\n",
                serde_json::to_string(&StreamEvent::Error(err.to_string()))?
            ))),
        })
    }
}

#[put("/compile")]
async fn compile(
    config: web::Data<AppConfig>,
    body: web::Json<CompileBody>,
    query: web::Query<CompileQuery>,
) -> Result<HttpResponse, Error> {
    let fresh_workspace = query.workspace_id.is_none();
    let workspace_id = query.workspace_id.clone().unwrap_or_else(|| {
        rand::rng()
            .sample_iter(&rand::distr::Alphabetic)
            .take(12)
            .map(char::from)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    });
    info!("compile for workspace ID: {workspace_id}");

    let streaming_responder = StreamingResponder::new();

    let workspace_path = Path::new(&config.workspaces_path).join(&workspace_id);
    if !fs::exists(&workspace_path)? {
        if !fresh_workspace {
            return Ok(HttpResponse::BadRequest().body("Invalid workspace ID"));
        }
        initialize_workspace(&workspace_path)?;
    }

    fs::write(Path::new(&workspace_path).join("src/lib.rs"), &body.lib_rs)?;
    fs::write(
        Path::new(&workspace_path).join("Cargo.toml"),
        &body.cargo_toml,
    )?;

    let mut child = streaming_responder.stream_command(
        Command::new("trunk")
            .arg("build")
            .current_dir(&workspace_path)
            .env("RUST_LOG", "info"),
    )?;

    let finish_sender = streaming_responder.new_sender();
    thread::spawn(move || {
        _ = finish_sender.send(match child.wait() {
            Ok(status) if status.success() => {
                StreamEvent::DownloadInfo(DownloadInfo {
                    js_download_url: Path::new("/workspaces")
                        .join(&workspace_id)
                        .join("dist/sheeet-lib.js")
                        .to_str()
                        .unwrap() // I build the complete path myself with UTF-8 chars only.
                        .into(),
                    wasm_download_url: Path::new("/workspaces")
                        .join(&workspace_id)
                        .join("dist/sheeet-lib_bg.wasm")
                        .to_str()
                        .unwrap() // I build the complete path myself with UTF-8 chars only.
                        .into(),
                    workspace_id,
                })
            }
            Ok(status) => StreamEvent::Error(format!("build failed: {status}")),
            Err(err) => StreamEvent::Error(format!("build failed with err: {err}")),
        })
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(streaming_responder.produce_stream()))
}

fn initialize_workspace(workspace_path: &PathBuf) -> Result<(), Error> {
    match fs::create_dir_all(&workspace_path) {
        Err(error) => {
            return Err(actix_web::error::ErrorInternalServerError(format!(
                "create dir all '{workspace_path:?}': {}",
                error.to_string()
            )));
        }
        _ => (),
    };

    match Command::new("cargo")
        .current_dir(workspace_path)
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
                "run cargo init: '{workspace_path:?}': {}",
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

    fs::write(
        Path::new(&workspace_path).join("index.html"),
        include_str!("user.html"),
    )?;

    Ok(())
}

struct AppConfig {
    workspaces_path: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    HttpServer::new(move || {
        let app_config = AppConfig {
            workspaces_path: env::var("WORKSPACES_PATH")
                .unwrap_or("/home/tikinang/workspaces".into()),
        };

        // TODO: CORS.
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();

        App::new()
            .service(Files::new("/workspaces", &app_config.workspaces_path))
            .app_data(web::Data::new(app_config))
            .wrap(cors)
            .service(compile)
    })
    .bind(("0.0.0.0", 8080))?
    .workers(4)
    .run()
    .await
}
