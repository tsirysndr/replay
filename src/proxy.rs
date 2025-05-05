use std::{process::exit, sync::Arc, thread, time::{SystemTime, UNIX_EPOCH}};

use http_body_util::{BodyExt, Full};
use hyper::{body::{Buf, Bytes, Incoming}, server::conn::http1, Request, Response};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::Mutex};
use hyper_util::rt::TokioIo;

use crate::{replay::start_replay_server, store::{save_logs_to_file, LogStore}};

pub const PROXY_LOG_FILE: &str = "replay_mocks.json";

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RequestLog {
    pub timestamp: u64,
    pub method: String,
    pub path: String,
    pub query_params: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ResponseLog {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ProxyLog {
  pub request: RequestLog,
  pub response: ResponseLog,
}

pub async fn start_server(target: &str, listen: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let target_uri = target.parse::<hyper::Uri>()?;
  let target_authority = target_uri.authority().ok_or("Invalid target URL")?;
  let target_scheme = target_uri.scheme_str().ok_or("http")?;
  let target_host = target_authority.host();
  let target_port = target_authority.port_u16().unwrap_or(if target_scheme == "https" { 443 } else { 80 });

  let logs = Arc::new(Mutex::new(Vec::<ProxyLog>::new()));
  let logs_for_saving = logs.clone();
  tokio::spawn(async move {
      loop {
          tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
          save_logs_to_file(&logs_for_saving, PROXY_LOG_FILE).await
                .unwrap_or_else(|e| eprintln!("Error saving logs to file: {}", e));
      }
  });

  let logs_for_replay = logs.clone();
  thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
      match start_replay_server(logs_for_replay, "127.0.0.1:6688").await {
        Ok(_) => {
          println!("Replay server stopped");
          exit(0);
        },
        Err(e) => eprintln!("Replay server error: {}", e),
      }
    });
  });

  let listener = TcpListener::bind(listen).await?;
  println!("Target URL: {}", target.magenta());
  println!("Proxy server is listening on {}", listen.green());
  println!("Replay server is running on {}", "127.0.0.1:6688".green());

  loop {
      let (stream, _) = listener.accept().await?;
      let io = TokioIo::new(stream);

      let target_host_str = target_host.to_string();
      let target_scheme = target_scheme.to_string();
      let logs_clone = logs.clone();

      tokio::task::spawn(async move {
          let service = hyper::service::service_fn(move |req: Request<Incoming>| {
              let target_host = target_host_str.clone();
              let scheme = target_scheme.clone();
              let logs = logs_clone.clone();

              async move {
                  proxy_handler(req, &target_host, target_port, &scheme, logs).await
              }
          });

          if let Err(err) = http1::Builder::new()
                .keep_alive(false)
                .max_buf_size(30 * 1024 * 1024)
              .serve_connection(io, service)
              .await
          {
              eprintln!("> Connection error: {}", err);
          }
      });
  }

}

pub async fn proxy_handler(
  req: Request<Incoming>,
  target_host: &str,
  target_port: u16,
  scheme: &str,
  logs: LogStore,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
  let timestamp = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_secs();

  let method = req.method().clone();
  let path = req.uri().path().to_string();
  let query = req.uri().query().map(|q| q.to_string());

  let headers: Vec<(String, String)> = req
      .headers()
      .iter()
      .map(|(name, value)| {
          (
              name.to_string(),
              value.to_str().unwrap_or("").to_string(),
          )
      })
      .collect();

  let (parts, body) = req.into_parts();
  let body_bytes = match body.collect().await {
      Ok(collected) => collected.aggregate(),
      Err(e) => {
          eprintln!("Error collecting request body: {}", e);
          return Ok(Response::builder()
              .status(500)
              .body(Full::new(Bytes::from("Internal Server Error")))
              .unwrap());
      }
  };

  let body_vec = body_bytes.chunk().to_vec();
  let body_str = String::from_utf8(body_vec.clone()).ok();

  let forward_uri = if target_port != 443 && target_port != 80 {
      format!(
          "{}://{}:{}{}{}",
          scheme,
          target_host,
          target_port,
          parts.uri.path(),
          parts.uri.query().map_or(String::new(), |q| format!("?{}", q))
      )
  } else {
      format!(
          "{}://{}{}{}",
          scheme,
          target_host,
          parts.uri.path(),
          parts.uri.query().map_or(String::new(), |q| format!("?{}", q))
      )
  };

  println!("{} {} {}", method.yellow(), path, forward_uri.magenta());

  let client = reqwest::Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .danger_accept_invalid_certs(true)
      .build()
      .unwrap_or_else(|_| reqwest::Client::new());

  let mut req_builder = match method.as_str() {
      "GET" => client.get(&forward_uri),
      "POST" => client.post(&forward_uri),
      "PUT" => client.put(&forward_uri),
      "DELETE" => client.delete(&forward_uri),
      "HEAD" => client.head(&forward_uri),
      "OPTIONS" => client.request(reqwest::Method::OPTIONS, &forward_uri),
      "PATCH" => client.patch(&forward_uri),
      _ => {
          eprintln!("Unsupported method: {}", method);
          return Ok(Response::builder()
              .status(400)
              .body(Full::new(Bytes::from("Bad Request: Unsupported Method")))
              .unwrap());
      }
  };

  for (name, value) in &headers {
      if name.to_lowercase() != "host" &&
         name.to_lowercase() != "connection" {
          if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) {
              if let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) {
                  req_builder = req_builder.header(header_name, header_value);
              }
          }
      }
  }

  if !body_vec.is_empty() {
      req_builder = req_builder.body(body_vec.clone());
  }

  let resp = match req_builder.send().await {
      Ok(resp) => resp,
      Err(e) => {
          eprintln!("Error sending request: {}", e);
          return Ok(Response::builder()
              .status(502)
              .body(Full::new(Bytes::from(format!("Bad Gateway: {}", e))))
              .unwrap());
      }
  };

  let status = resp.status().as_u16();

  let resp_headers: Vec<(String, String)> = resp
      .headers()
      .iter()
      .map(|(name, value)| {
          (
              name.to_string(),
              value.to_str().unwrap_or("").to_string(),
          )
      })
      .collect();

  let resp_bytes = match resp.bytes().await {
      Ok(bytes) => bytes,
      Err(e) => {
          eprintln!("Error reading response body: {}", e);
          return Ok(Response::builder()
              .status(500)
              .body(Full::new(Bytes::from("Internal Server Error")))
              .unwrap());
      }
  };

  let resp_vec = resp_bytes.to_vec();
  let resp_str = String::from_utf8(resp_vec.clone()).ok();

  let log_entry = ProxyLog {
      request: RequestLog {
          timestamp,
          method: method.to_string(),
          path,
          query_params: query,
          headers,
          body: body_str,
      },
      response: ResponseLog {
          status,
          headers: resp_headers.clone(),
          body: resp_str.clone(),
      },
  };

  {
    let mut logs_guard = logs.lock().await;
    if !logs_guard.iter()
      .any(|log|
          log.request.method == log_entry.request.method &&
          log.request.path == log_entry.request.path &&
          log.request.query_params == log_entry.request.query_params
      ) {
        logs_guard.push(log_entry.clone());
    }
  }

  println!("Saved request/response to log store {}", PROXY_LOG_FILE.magenta());

  let mut builder = Response::builder().status(status);

  for (name, value) in resp_headers {
      if name.to_lowercase() != "connection" &&
         name.to_lowercase() != "transfer-encoding" {
          if let Ok(header_name) = hyper::header::HeaderName::from_bytes(name.as_bytes()) {
              if let Ok(header_value) = hyper::header::HeaderValue::from_str(&value) {
                  builder = builder.header(header_name, header_value);
              }
          }
      }
  }

  builder = builder.header("content-length", resp_vec.len());
  builder = builder.header("connection", "close");

  Ok(builder
      .body(Full::new(Bytes::from(resp_vec)))
      .unwrap_or_else(|_| {
          Response::builder()
              .status(500)
              .body(Full::new(Bytes::from("Internal Server Error")))
              .unwrap()
      }))
}