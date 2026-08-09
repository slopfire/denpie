#!/usr/bin/env python3
"""Small dependency-light Denpie API v1 client."""

from __future__ import annotations

import os
import sys
import uuid
from urllib.error import HTTPError
from urllib.request import Request, urlopen

import denpie_pb2 as pb


def api_call(envelope: pb.ApiV1Request) -> pb.ApiV1Response:
    endpoint = os.environ.get("DENPIE_URL", "http://127.0.0.1:3017/api/v1")
    headers = {"Content-Type": "application/x-protobuf"}
    api_key = os.environ.get("DENPIE_API_KEY")
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    request = Request(
        endpoint,
        data=envelope.SerializeToString(),
        headers=headers,
        method="POST",
    )
    try:
        with urlopen(request, timeout=30) as response:
            status = response.status
            body = response.read()
    except HTTPError as error:
        status = error.code
        body = error.read()

    result = pb.ApiV1Response()
    result.ParseFromString(body)
    if result.WhichOneof("outcome") == "error":
        code = pb.ApiErrorCode.Name(result.error.code)
        raise RuntimeError(
            f"Denpie returned HTTP {status} {code}: {result.error.message} "
            f"(request_id={result.request_id}, retryable={result.error.retryable})"
        )
    if status < 200 or status >= 300 or result.WhichOneof("outcome") != "success":
        raise RuntimeError(f"invalid Denpie response: HTTP {status}")
    return result


def get_info() -> pb.ApiV1Response:
    return api_call(
        pb.ApiV1Request(
            request_id="python-get-api-info",
            call=pb.ApiRequest(get_api_info=pb.Empty()),
        )
    )


def list_cards() -> pb.ApiV1Response:
    return api_call(
        pb.ApiV1Request(
            request_id="python-list-flow-cards",
            call=pb.ApiRequest(
                list_flow_cards=pb.ListFlowCardsRequest(page_size=12)
            ),
        )
    )


def create_document(idempotency_key: str) -> pb.ApiV1Response:
    return api_call(
        pb.ApiV1Request(
            request_id="python-create-document",
            idempotency_key=idempotency_key,
            call=pb.ApiRequest(
                create_document=pb.AddDocumentRequest(
                    source_type="document",
                    title="Python API example",
                    content="Created by the checked-in Python client example.",
                )
            ),
        )
    )


def self_test() -> None:
    request = pb.ApiV1Request(
        request_id="python-self-test",
        call=pb.ApiRequest(get_api_info=pb.Empty()),
    )
    decoded = pb.ApiV1Request.FromString(request.SerializeToString())
    assert decoded.call.WhichOneof("op") == "get_api_info"
    print("Python protobuf client self-test passed")


def main() -> int:
    command = sys.argv[1] if len(sys.argv) > 1 else "info"
    if command == "--self-test":
        self_test()
        return 0
    if command == "info":
        response = get_info()
    elif command == "cards":
        response = list_cards()
    elif command == "create-document":
        key = os.environ.get("DENPIE_IDEMPOTENCY_KEY", str(uuid.uuid4()))
        print(f"idempotency_key={key}", file=sys.stderr)
        response = create_document(key)
    else:
        print("usage: denpie_client.py [info|cards|create-document|--self-test]", file=sys.stderr)
        return 2
    print(response)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
