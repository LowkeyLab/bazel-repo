from __future__ import annotations

import unittest

import grpc

from pinyin_hanzi import hanzi_pb2
from pinyin_hanzi.rpc import create_hanzi_guesser_stub
from pinyin_hanzi.server import create_server


class HanziGuesserRpcTest(unittest.TestCase):
    def setUp(self) -> None:
        self.server = create_server()
        self.port = self.server.add_insecure_port("127.0.0.1:0")
        self.server.start()
        self.channel = grpc.insecure_channel(f"127.0.0.1:{self.port}")
        grpc.channel_ready_future(self.channel).result(timeout=5)
        self.guess_hanzi = create_hanzi_guesser_stub(self.channel)

    def tearDown(self) -> None:
        self.channel.close()
        stop_event = self.server.stop(0)
        stop_event.wait(timeout=5)

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
