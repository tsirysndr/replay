use crate::proxy::PROXY_LOG_FILE;
use crate::store::{LogStore, load_logs_from_file};
use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, web};
use owo_colors::OwoColorize;

pub async fn start_replay_server(
    logs: LogStore,
    bind_address: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(file_logs) = load_logs_from_file(PROXY_LOG_FILE) {
        let mut logs_guard = logs.lock().await;
        for log in file_logs {
            logs_guard.push(log);
        }
    }

    fn build_request_key(method: &str, path: &str, query: &Option<String>) -> String {
        if let Some(q) = query {
            format!("{} {}?{}", method, path, q)
        } else {
            format!("{} {}", method, path)
        }
    }

    async fn replay_handler(
        req: HttpRequest,
        _body: web::Bytes,
        logs: web::Data<LogStore>,
    ) -> impl Responder {
        let method = req.method().to_string();
        let path = req.path().to_string();
        let query = req.query_string();
        let query_opt = if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        };

        let key = build_request_key(&method, &path, &query_opt);
        println!("Replay server received request: {}", key.magenta());

        let response = {
            let logs_guard = logs.lock().await;
            logs_guard
                .iter()
                .find(|log| {
                    let log_key = build_request_key(
                        &log.request.method,
                        &log.request.path,
                        &log.request.query_params,
                    );
                    log_key == key
                })
                .cloned()
        };

        if let Some(log) = response {
            println!("Found matching response for: {}", key.magenta());

            let mut response_builder = HttpResponse::build(
                actix_web::http::StatusCode::from_u16(log.response.status)
                    .unwrap_or(actix_web::http::StatusCode::OK),
            );

            for (name, value) in log.response.headers {
                if let (Ok(header_name), Ok(header_value)) = (
                    HeaderName::try_from(name.as_str()),
                    HeaderValue::try_from(value.as_str()),
                ) {
                    response_builder.append_header((header_name, header_value));
                }
            }

            if let Some(body) = log.response.body {
                response_builder.body(body)
            } else {
                response_builder.finish()
            }
        } else {
            println!("No matching response found for: {}", key.magenta());
            HttpResponse::NotFound().body("No matching request found in logs")
        }
    }

    async fn list_requests(logs: web::Data<LogStore>) -> impl Responder {
        let logs_guard = logs.lock().await;
        let requests: Vec<_> = logs_guard
            .iter()
            .map(|log| {
                let key = build_request_key(
                    &log.request.method,
                    &log.request.path,
                    &log.request.query_params,
                );
                (key, log.response.status)
            })
            .collect();

        web::Json(requests)
    }

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(logs.clone()))
            .route("/admin/requests", web::get().to(list_requests))
            .default_service(web::route().to(replay_handler))
    })
    .bind(bind_address)?
    .run()
    .await?;

    Ok(())
}
