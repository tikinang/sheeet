use actix_cors::Cors;
use actix_files::Files;
use actix_web::{put, web, App, Error, HttpResponse, HttpServer};
use bytes::Bytes;
use futures_util::stream::{self, StreamExt};
use futures_util::Stream;
use log::info;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs, thread};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

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
    Log(String),
    DownloadInfo(DownloadInfo),
}

struct StreamingResponder {
    receiver: Option<UnboundedReceiver<StreamEvent>>,
    sender: UnboundedSender<StreamEvent>,
}

impl StreamingResponder {
    fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        StreamingResponder {
            receiver: Some(receiver),
            sender,
        }
    }

    fn send_event(&self, stream_event: StreamEvent) {
        _ = self.sender.send(stream_event);
    }

    fn log(&self, line: String) {
        _ = self.sender.send(StreamEvent::Log(line));
    }

    fn terminate_error(&self, err: impl ToString) {
        _ = self.sender.send(StreamEvent::Error(err.to_string()));
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
        let sender = self.sender.clone();
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

    // TODO: Update docs.
    /// Consumes the responder and produces stream of all messages that receiver will receive.
    fn produce_stream(&mut self) -> impl Stream<Item = Result<Bytes, Error>> + 'static {
        stream::unfold(self.receiver.take().unwrap(), |mut receiver| async move {
            match receiver.recv().await {
                Some(event) => Some((event, receiver)),
                None => None,
            }
        })
        .map(|event: StreamEvent| {
            Ok(Bytes::from(format!(
                "data: {}\n",
                serde_json::to_string(&event)?
            )))
        })
    }
}

#[put("/compile")]
async fn compile(
    config: web::Data<AppConfig>,
    body: web::Json<CompileBody>,
    query: web::Query<CompileQuery>,
) -> Result<HttpResponse, Error> {
    let workspace_id = query.workspace_id.clone().unwrap_or_else(|| {
        rand::rng()
            .sample_iter(&rand::distr::Alphabetic)
            .take(12)
            .map(char::from)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    });
    info!("compile for workspace ID: {workspace_id}");
    let workspace_path = Path::new(&config.workspaces_path).join(&workspace_id);
    if !fs::exists(&workspace_path)? {
        if query.workspace_id.is_some() {
            return Ok(HttpResponse::NotFound().body("Invalid workspace ID"));
        }
    }

    let mut responder = StreamingResponder::new();
    let stream = responder.produce_stream();
    thread::spawn(move || {
        if !fs::exists(&workspace_path).unwrap_or(false) {
            if let Err(err) = fs::create_dir_all(&workspace_path) {
                responder.terminate_error(format!("create dir all '{workspace_path:?}': {err}"));
                return;
            };

            let mut child = match responder.stream_command(
                Command::new("cargo")
                    .current_dir(&workspace_path)
                    .arg("init")
                    .arg("--lib")
                    .arg("--name")
                    .arg("sheeet-lib")
                    .env("RUST_LOG", "info")
                    .env("RUST_LOG_STYLE", "never"),
            ) {
                Ok(child) => child,
                Err(err) => {
                    responder.terminate_error(err);
                    return;
                }
            };

            match child.wait() {
                Ok(status) if status.success() => {
                    responder.log("Workspace initialized.".into());
                }
                Ok(status) => {
                    responder.terminate_error(format!("build failed: {status}"));
                    return;
                }
                Err(err) => {
                    responder.terminate_error(format!("build failed with err: {err}"));
                    return;
                }
            };
        }

        if let Err(err) = fs::write(
            Path::new(&workspace_path).join("index.html"),
            include_str!("user.html"),
        ) {
            responder.terminate_error(err);
            return;
        };
        if let Err(err) = fs::write(Path::new(&workspace_path).join("src/lib.rs"), &body.lib_rs) {
            responder.terminate_error(err);
            return;
        };
        if let Err(err) = fs::write(
            Path::new(&workspace_path).join("Cargo.toml"),
            &body.cargo_toml,
        ) {
            responder.terminate_error(err);
            return;
        };

        let mut child = match responder.stream_command(
            Command::new("trunk")
                .arg("build")
                .current_dir(&workspace_path)
                .env("RUST_LOG", "info")
                .env("RUST_LOG_STYLE", "never"),
        ) {
            Ok(child) => child,
            Err(err) => {
                responder.terminate_error(err);
                return;
            }
        };

        match child.wait() {
            Ok(status) if status.success() => {
                responder.send_event(StreamEvent::DownloadInfo(DownloadInfo {
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
                }));
            }
            Ok(status) => {
                responder.terminate_error(format!("build failed: {status}"));
                return;
            }
            Err(err) => {
                responder.terminate_error(format!("build failed with err: {err}"));
                return;
            }
        };
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(stream))
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
