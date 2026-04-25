from __future__ import annotations

import unittest

from pinyin_hanzi.service import DEFAULT_TOP_K, MAX_TOP_K, InvalidPinyinError, guess_hanzi_candidates


class GuessHanziCandidatesTest(unittest.TestCase):
    def test_returns_likely_candidates(self) -> None:
        candidates = guess_hanzi_candidates(["ni", "hao"], top_k=3)

        self.assertGreaterEqual(len(candidates), 1)
        self.assertIn("你好", [candidate.hanzi for candidate in candidates])

    def test_normalizes_nonstandard_pinyin(self) -> None:
        candidates = guess_hanzi_candidates(["lue"], top_k=1)

        self.assertEqual(len(candidates), 1)
        self.assertTrue(candidates[0].hanzi)

    def test_defaults_top_k_when_zero(self) -> None:
        candidates = guess_hanzi_candidates(["ni", "hao"], top_k=0)

        self.assertLessEqual(len(candidates), DEFAULT_TOP_K)

    def test_caps_top_k(self) -> None:
        candidates = guess_hanzi_candidates(["ni", "hao"], top_k=MAX_TOP_K + 50)

        self.assertLessEqual(len(candidates), MAX_TOP_K)

    def test_rejects_invalid_pinyin(self) -> None:
        with self.assertRaisesRegex(InvalidPinyinError, "invalid pinyin"):
            guess_hanzi_candidates(["zhii"])

    def test_rejects_empty_input(self) -> None:
        with self.assertRaisesRegex(InvalidPinyinError, "at least one syllable"):
            guess_hanzi_candidates([])


if __name__ == "__main__":
    unittest.main()
