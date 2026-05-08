use axum::{
    http::header,
    response::{IntoResponse, Response},
};
use prost::Message;

use super::{pb, types::TipCardJson};

pub(crate) fn protobuf_response<T: Message>(msg: &T) -> Response {
    let mut buf = bytes::BytesMut::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).unwrap();
    (
        [(header::CONTENT_TYPE, "application/x-protobuf")],
        buf.freeze(),
    )
        .into_response()
}

pub(crate) fn empty_response() -> pb::ApiResponse {
    pb::ApiResponse {
        result: Some(pb::api_response::Result::Ok(pb::Empty {})),
    }
}

pub(crate) fn tip_response_json(
    id: i64,
    topic: &str,
    full_content: String,
    compressed_content: String,
    image_data: Vec<String>,
    tipcard_type: String,
    pinned: bool,
) -> TipCardJson {
    TipCardJson {
        id,
        topic: topic.to_string(),
        full_content,
        compressed_content,
        image_data,
        tipcard_type,
        pinned,
    }
}

pub(crate) fn tip_to_pb(card: TipCardJson) -> pb::TipCardResponse {
    pb::TipCardResponse {
        id: card.id,
        topic: card.topic,
        full_content: card.full_content,
        compressed_content: card.compressed_content,
        tipcard_type: card.tipcard_type,
        pinned: card.pinned,
    }
}
