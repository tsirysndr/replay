use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

use crate::proxy::ProxyLog;

pub type LogStore = Arc<Mutex<Vec<ProxyLog>>>;

pub fn save_logs_to_file(
    logs: &LogStore,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let logs_guard = logs.lock().unwrap();

    if logs_guard.is_empty() {
        return Ok(());
    }

    let json = serde_json::to_string_pretty(&*logs_guard)?;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(filename)?;

    file.write_all(json.as_bytes())?;

    Ok(())
}

pub fn load_logs_from_file(
    filename: &str,
) -> Result<Vec<ProxyLog>, Box<dyn std::error::Error + Send + Sync>> {
    if !Path::new(filename).exists() {
        return Ok(Vec::new());
    }

    let file = File::open(filename)?;
    let logs: Vec<ProxyLog> = serde_json::from_reader(file)?;

    Ok(logs)
}
