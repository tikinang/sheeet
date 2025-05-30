use actix_cors::Cors;
use actix_files::Files;
use actix_web::{post, put, web, App, HttpResponse, HttpServer, Responder};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::{Command, Stdio};

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

// TODO: Take from env.
const WORKSPACES_PATH: &str = "/home/tikinang/code/other/sheeet/workspaces";

#[put("/compile")]
async fn compile(body: web::Json<CompileBody>, query: web::Query<CompileQuery>) -> impl Responder {
    // TODO: Error handling.

    println!("workspace ID: {}", &query.workspace_id);
    let path = format!("{}/{}", WORKSPACES_PATH, &query.workspace_id);

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
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
        Ok(status) => println!("{}", status),
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
    let rel_path = format! {"{}/{}", WORKSPACES_PATH, &workspace_id};

    match fs::create_dir_all(&rel_path) {
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
        _ => (),
    };

    match Command::new("cargo")
        .current_dir(&rel_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg("init")
        .arg("--lib")
        .arg("--name")
        .arg("sheeet-lib")
        .status()
    {
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
        Ok(status) => println!("{}", status),
    };

    HttpResponse::Ok().json(InitializeResponse { workspace_id })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        // TODO: CORS.
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();

        App::new()
            .service(Files::new("/workspaces", WORKSPACES_PATH))
            .wrap(cors)
            .service(compile)
            .service(initialize)
    })
    .bind(("127.0.0.1", 8080))?
    .workers(4)
    .run()
    .await
}
