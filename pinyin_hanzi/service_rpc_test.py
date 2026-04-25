from __future__ import annotations

import socket
import unittest

import grpc

from pinyin_hanzi import hanzi_pb2
from pinyin_hanzi.server import create_server


def _pick_unused_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


class HanziGuesserRpcTest(unittest.TestCase):
    def setUp(self) -> None:
        self.port = _pick_unused_port()
        self.server = create_server()
        self.server.add_insecure_port(f"127.0.0.1:{self.port}")
        self.server.start()
        self.channel = grpc.insecure_channel(f"127.0.0.1:{self.port}")
        self.guess_hanzi = self.channel.unary_unary(
            "/pinyin_hanzi.v1.HanziGuesser/GuessHanzi",
            request_serializer=hanzi_pb2.GuessHanziRequest.SerializeToString,
            response_deserializer=hanzi_pb2.GuessHanziResponse.FromString,
        )

    def tearDown(self) -> None:
        self.channel.close()
        self.server.stop(None)

    def test_returns_candidates_over_grpc(self) -> None:
        response = self.guess_hanzi(
            hanzi_pb2.GuessHanziRequest(pinyin_words=["ni", "hao"], top_k=3)
        )

        self.assertGreaterEqual(len(response.candidates), 1)
        self.assertIn("你好", [candidate.hanzi for candidate in response.candidates])

    def test_rejects_invalid_pinyin_over_grpc(self) -> None:
        with self.assertRaises(grpc.RpcError) as ctx:
            self.guess_hanzi(hanzi_pb2.GuessHanziRequest(pinyin_words=["zhii"]))

        self.assertEqual(ctx.exception.code(), grpc.StatusCode.INVALID_ARGUMENT)


if __name__ == "__main__":
    unittest.main()
