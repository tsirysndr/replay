
use std::sync::{Arc, Mutex};

use clap::{Arg, Command};
use owo_colors::OwoColorize;
use proxy::start_server;

pub mod proxy;
pub mod replay;
pub mod store;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const BANNER: &str = r#"

        ____             __
       / __ \___  ____  / /___ ___  __
      / /_/ / _ \/ __ \/ / __ `/ / / /
     / _, _/  __/ /_/ / / /_/ / /_/ /
    /_/ |_|\___/ .___/_/\__,_/\__, /
              /_/            /____/

    Sniff and replay HTTP requests and responses — perfect for mocking APIs during testing.
    "#;
    let matches = Command::new("replay")
        .version("0.1.0")
        .author("Tsiry Sandratraina <tsiry.sndr@rocksky.app>")
        .about(&format!("{}", BANNER.magenta()))
        .arg(
            Arg::new("target")
                .short('t')
                .long("target")
                .help("The target URL to replay the requests to")
        )
        .arg(
            Arg::new("listen")
                .short('l')
                .long("listen")
                .help("The address to listen on for incoming requests")
                .default_value("127.0.0.1:6677")
        )
        .subcommand(
            Command::new("mock")
                .about("Read mocks from replay_mock.json and start a replay server")
        )
        .get_matches();


    if let Some(_) = matches.subcommand_matches("mock") {
        let logs = store::load_logs_from_file(proxy::PROXY_LOG_FILE)?;
        let logs = Arc::new(Mutex::new(logs));
        let listen = matches.get_one::<String>("listen").unwrap();
        println!("Loaded {} mocks from {}", logs.lock().unwrap().len().magenta(), proxy::PROXY_LOG_FILE.magenta());
        println!("Replay server is running on {}", listen.magenta());
        replay::start_replay_server(logs, listen).await?;
        return Ok(());
    }


    let target = matches.get_one::<String>("target");

    if target.is_none() {
        eprintln!("Error: Target URL is required");
        std::process::exit(1);
    }

    let target = target.unwrap();
    let listen = matches.get_one::<String>("listen").unwrap();

    start_server(target, listen).await?;

    Ok(())
}
