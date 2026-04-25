from __future__ import annotations

import logging
import os
from concurrent import futures

import grpc

from pinyin_hanzi import hanzi_pb2
from pinyin_hanzi.service import HanziGuesserService


def create_server() -> grpc.Server:
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
    service = HanziGuesserService()
    server.add_generic_rpc_handlers(
        (
            grpc.method_handlers_generic_handler(
                "pinyin_hanzi.v1.HanziGuesser",
                {
                    "GuessHanzi": grpc.unary_unary_rpc_method_handler(
                        service.GuessHanzi,
                        request_deserializer=hanzi_pb2.GuessHanziRequest.FromString,
                        response_serializer=hanzi_pb2.GuessHanziResponse.SerializeToString,
                    ),
                },
            ),
        )
    )
    return server


def serve(port: int) -> None:
    server = create_server()
    server.add_insecure_port(f"[::]:{port}")
    server.start()
    logging.info("pinyin_hanzi gRPC server listening on %s", port)
    server.wait_for_termination()


def main() -> None:
    logging.basicConfig(level=logging.INFO)
    port = int(os.environ.get("PORT", "50051"))
    serve(port)


if __name__ == "__main__":
    main()
