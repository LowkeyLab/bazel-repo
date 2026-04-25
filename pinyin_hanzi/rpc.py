from __future__ import annotations

import grpc

from pinyin_hanzi import hanzi_pb2

SERVICE_NAME = "pinyin_hanzi.v1.HanziGuesser"
GUESS_HANZI_METHOD = f"/{SERVICE_NAME}/GuessHanzi"


def add_hanzi_guesser_to_server(
    service: object,
    server: grpc.Server,
) -> None:
    server.add_generic_rpc_handlers(
        (
            grpc.method_handlers_generic_handler(
                SERVICE_NAME,
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


def create_hanzi_guesser_stub(channel: grpc.Channel) -> grpc.UnaryUnaryMultiCallable:
    return channel.unary_unary(
        GUESS_HANZI_METHOD,
        request_serializer=hanzi_pb2.GuessHanziRequest.SerializeToString,
        response_deserializer=hanzi_pb2.GuessHanziResponse.FromString,
    )
