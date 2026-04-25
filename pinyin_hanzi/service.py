from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

import grpc
from Pinyin2Hanzi import DefaultDagParams, dag, is_pinyin, simplify_pinyin

from pinyin_hanzi import hanzi_pb2

DEFAULT_TOP_K = 5
MAX_TOP_K = 20
_DAG_PARAMS = DefaultDagParams()


class InvalidPinyinError(ValueError):
    """Raised when the request contains invalid pinyin syllables."""


@dataclass(frozen=True)
class Candidate:
    hanzi: str
    score: float
    segments: tuple[str, ...]


def _normalize_pinyin(words: Sequence[str]) -> list[str]:
    normalized = [simplify_pinyin(word.strip().lower()) for word in words]

    if not normalized:
        raise InvalidPinyinError("pinyin_words must contain at least one syllable")

    invalid_words = [word for word in normalized if not word or not is_pinyin(word)]
    if invalid_words:
        raise InvalidPinyinError(
            "invalid pinyin syllable(s): " + ", ".join(sorted(set(invalid_words)))
        )

    return normalized


def _normalize_top_k(top_k: int) -> int:
    if top_k <= 0:
        return DEFAULT_TOP_K
    return min(top_k, MAX_TOP_K)


def guess_hanzi_candidates(pinyin_words: Sequence[str], top_k: int = DEFAULT_TOP_K) -> list[Candidate]:
    normalized_words = _normalize_pinyin(pinyin_words)
    candidate_limit = _normalize_top_k(top_k)

    return [
        Candidate(
            hanzi="".join(item.path),
            score=float(item.score),
            segments=tuple(item.path),
        )
        for item in dag(_DAG_PARAMS, normalized_words, path_num=candidate_limit, log=True)
    ]


class HanziGuesserService:
    def GuessHanzi(
        self,
        request: hanzi_pb2.GuessHanziRequest,
        context: grpc.ServicerContext,
    ) -> hanzi_pb2.GuessHanziResponse:
        try:
            candidates = guess_hanzi_candidates(request.pinyin_words, request.top_k)
        except InvalidPinyinError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return hanzi_pb2.GuessHanziResponse()

        return hanzi_pb2.GuessHanziResponse(
            candidates=[
                hanzi_pb2.HanziCandidate(
                    hanzi=candidate.hanzi,
                    score=candidate.score,
                    segments=list(candidate.segments),
                )
                for candidate in candidates
            ]
        )
