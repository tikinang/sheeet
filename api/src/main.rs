use actix_cors::Cors;
use actix_files::Files;
use actix_web::{post, put, web, App, HttpResponse, HttpServer, Responder};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::{env, fs};

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
struct CompileResponse {
    js_download_url: String,
    wasm_download_url: String,
}

#[put("/compile")]
async fn compile(body: web::Json<CompileBody>, query: web::Query<CompileQuery>) -> impl Responder {
    // TODO: Error handling.

    println!("workspace ID: {}", &query.workspace_id);
    let path = format!(
        "{}/{}",
        env::var(ENV_KEY_API_WORKSPACES_PATH).unwrap_or(DEFAULT_WORKSPACES_PATH.into()),
        &query.workspace_id
    );

    if !fs::exists(&path).unwrap() {
        // TODO: This is wrong and temporary. Users should not be allowed to use their own workspace IDs.
        if let Err(err) = initialize_workspace(&query.workspace_id) {
            return err;
        };
    }

    fs::write(format!("{path}/src/lib.rs"), body.lib_rs.as_str()).unwrap();
    fs::write(format!("{path}/Cargo.toml"), body.cargo_toml.as_str()).unwrap();
    fs::write(format!("{path}/index.html"), include_str!("user.html")).unwrap();

    println!("workspace PATH: {}", path);
    match Command::new("trunk")
        .current_dir(&path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg("build")
        .status()
    {
        Err(error) => {
            return HttpResponse::InternalServerError()
                .body(format!("run trunk build: '{path}': {}", error.to_string()));
        }
        Ok(status) => {
            if !status.success() {
                return HttpResponse::InternalServerError()
                    .body(format!("run trunk build: exit status: {:?}", status));
            }
        }
    };

    HttpResponse::Ok().json(CompileResponse {
        js_download_url: format!("/workspaces/{}/dist/sheeet-lib.js", query.workspace_id),
        wasm_download_url: format!("/workspaces/{}/dist/sheeet-lib_bg.wasm", query.workspace_id),
    })
}

#[derive(Serialize)]
struct InitializeResponse {
    workspace_id: String,
}

#[post("/initialize")]
async fn initialize() -> impl Responder {
    // TODO: Error handling.

    let workspace_id: String = rand::rng()
        .sample_iter(&rand::distr::Alphabetic)
        .take(12)
        .map(char::from)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    println!("{}", workspace_id);

    if let Err(err) = initialize_workspace(&workspace_id) {
        return err;
    };

    HttpResponse::Ok().json(InitializeResponse { workspace_id })
}

fn initialize_workspace(workspace_id: &str) -> Result<(), HttpResponse> {
    let path = format! {"{}/{}", env::var(ENV_KEY_API_WORKSPACES_PATH).unwrap_or(DEFAULT_WORKSPACES_PATH.into()), &workspace_id};

    match fs::create_dir_all(&path) {
        Err(error) => {
            return Err(HttpResponse::InternalServerError()
                .body(format!("create dir all '{path}': {}", error.to_string())));
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
            return Err(HttpResponse::InternalServerError()
                .body(format!("run cargo init: '{path}': {}", error.to_string())));
        }
        Ok(status) => {
            if !status.success() {
                return Err(HttpResponse::InternalServerError()
                    .body(format!("run cargo init: exit status: {:?}", status)));
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
