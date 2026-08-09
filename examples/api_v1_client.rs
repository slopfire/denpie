use std::{env, error::Error};

use prost::Message;

mod pb {
    include!(concat!(env!("OUT_DIR"), "/denpie.rs"));
}

fn info_request() -> pb::ApiV1Request {
    pb::ApiV1Request {
        request_id: "rust-get-api-info".to_string(),
        call: Some(pb::ApiRequest {
            auth: String::new(),
            op: Some(pb::api_request::Op::GetApiInfo(pb::Empty {})),
        }),
        idempotency_key: String::new(),
    }
}

fn cards_request() -> pb::ApiV1Request {
    pb::ApiV1Request {
        request_id: "rust-list-flow-cards".to_string(),
        call: Some(pb::ApiRequest {
            auth: String::new(),
            op: Some(pb::api_request::Op::ListFlowCards(
                pb::ListFlowCardsRequest {
                    page_size: 12,
                    page_token: String::new(),
                },
            )),
        }),
        idempotency_key: String::new(),
    }
}

fn create_document_request(idempotency_key: String) -> pb::ApiV1Request {
    pb::ApiV1Request {
        request_id: "rust-create-document".to_string(),
        call: Some(pb::ApiRequest {
            auth: String::new(),
            op: Some(pb::api_request::Op::CreateDocument(
                pb::AddDocumentRequest {
                    topic_id_opt: String::new(),
                    source_type: "document".to_string(),
                    title: "Rust API example".to_string(),
                    url: String::new(),
                    content: "Created by the checked-in Rust client example.".to_string(),
                    topic_ids: Vec::new(),
                },
            )),
        }),
        idempotency_key,
    }
}

async fn api_call(request: pb::ApiV1Request) -> Result<pb::ApiV1Response, Box<dyn Error>> {
    let endpoint =
        env::var("DENPIE_URL").unwrap_or_else(|_| "http://127.0.0.1:3017/api/v1".to_string());
    let client = reqwest::Client::new();
    let mut builder = client
        .post(endpoint)
        .header("Content-Type", "application/x-protobuf")
        .body(request.encode_to_vec());
    if let Ok(api_key) = env::var("DENPIE_API_KEY") {
        builder = builder.bearer_auth(api_key);
    }
    let response = builder.send().await?;
    let status = response.status();
    let decoded = pb::ApiV1Response::decode(response.bytes().await?)?;
    match decoded.outcome.as_ref() {
        Some(pb::api_v1_response::Outcome::Success(_)) if status.is_success() => Ok(decoded),
        Some(pb::api_v1_response::Outcome::Error(error)) => Err(format!(
            "Denpie returned HTTP {status}, code {}, retryable={}: {} (request_id={})",
            error.code, error.retryable, error.message, decoded.request_id
        )
        .into()),
        _ => Err(format!("invalid Denpie response: HTTP {status}").into()),
    }
}

fn self_test() -> Result<(), Box<dyn Error>> {
    let encoded = info_request().encode_to_vec();
    let decoded = pb::ApiV1Request::decode(encoded.as_slice())?;
    let operation = decoded.call.and_then(|call| call.op);
    if !matches!(operation, Some(pb::api_request::Op::GetApiInfo(_))) {
        return Err("operation did not round-trip".into());
    }
    println!("Rust protobuf client self-test passed");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let command = env::args().nth(1).unwrap_or_else(|| "info".to_string());
    if command == "--self-test" {
        return self_test();
    }
    let request = match command.as_str() {
        "info" => info_request(),
        "cards" => cards_request(),
        "create-document" => {
            let key = env::var("DENPIE_IDEMPOTENCY_KEY").map_err(
                |_| "set DENPIE_IDEMPOTENCY_KEY to a UUID persisted with this logical mutation",
            )?;
            create_document_request(key)
        }
        _ => return Err("usage: api_v1_client [info|cards|create-document|--self-test]".into()),
    };
    println!("{:#?}", api_call(request).await?);
    Ok(())
}
