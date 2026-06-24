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
