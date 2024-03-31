mod agent;
mod error;
mod handler;
mod option;
mod worker;

#[macro_use]
extern crate serde_derive;
extern crate lazy_static;

use std::{fs, path::PathBuf};

use actix_web::{App, HttpServer};
use agent::{platform, rclone::RcloneClient};
use option::JudgerCommad;
use worker::JudgeWorker;

#[actix_web::main]
// The button provided by rust-analyzer will not work as expected here
// Use RUN AND DEBUG feature in VSCode
async fn main() -> std::io::Result<()> {
    let opt = option::load_option();

    let maybe_rclone_client = if opt.enable_rclone {
        Some(agent::rclone::RcloneClient::new(
            opt.rclone_config_path.clone(),
        ))
    } else {
        None
    };

    match opt.cmd {
        option::JudgerCommad::Serve {
            platform_uri,
            fetch_task_interval,
            port,
        } => {
            serve(
                maybe_rclone_client,
                opt.problem_package_bucket,
                opt.problem_package_dir,
                platform_uri.clone(),
                fetch_task_interval,
                port,
            )
            .await
        }
        JudgerCommad::Judge {
            problem_slug,
            language,
            src_path,
        } => judge(
            maybe_rclone_client,
            opt.problem_package_bucket,
            opt.problem_package_dir,
            problem_slug,
            language,
            src_path,
        ),
    }
}

async fn serve(
    maybe_rclone_client: Option<RcloneClient>,
    problem_package_bucket: String,
    problem_package_dir: PathBuf,
    platform_uri: String,
    fetch_task_interval: u64,
    port: u16,
) -> std::io::Result<()> {
    let platform_client = platform::PlatformClient::new(platform_uri.clone());

    let worker = match JudgeWorker::new(
        Some(platform_client),
        maybe_rclone_client,
        fetch_task_interval,
        problem_package_bucket.clone(),
        problem_package_dir.clone(),
    ) {
        Ok(worker) => worker,
        Err(e) => {
            log::error!("Failed to create worker: {:?}", e);
            return Ok(());
        }
    };
    tokio::spawn(async move { worker.run().await });

    HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .configure(handler::route)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

fn judge(
    maybe_rclone_client: Option<RcloneClient>,
    problem_package_bucket: String,
    problem_package_dir: PathBuf,
    problem_slug: String,
    language: judge_core::compiler::Language,
    src_path: std::path::PathBuf,
) -> std::io::Result<()> {
    // Read code from src_path
    let code = match fs::read_to_string(src_path) {
        Ok(code) => code,
        Err(e) => {
            log::error!("Failed to read code from src_path: {:?}", e);
            return Ok(());
        }
    };

    let worker = match JudgeWorker::new(
        None,
        maybe_rclone_client,
        0,
        problem_package_bucket.clone(),
        problem_package_dir.clone(),
    ) {
        Ok(worker) => worker,
        Err(e) => {
            log::error!("Failed to create worker: {:?}", e);
            return Ok(());
        }
    };

    match worker.run_judge(problem_slug, language, code) {
        Ok(result) => {
            println!("{:?}", result);
        }
        Err(e) => {
            log::error!("Failed to judge task: {:?}", e);
        }
    }
    Ok(())
}
