package io.lowkeylab.guesstheword.game

import io.lowkeylab.guesstheword.player.PlayerId
import kotlinx.serialization.Serializable

@JvmInline
@Serializable
value class GameId(
    val id: String,
)

@Serializable
class Game(
    val id: GameId,
    val rounds: List<Round> = emptyList(),
)

@Serializable
class Round(
    val id: UInt,
    val guesses: Map<PlayerId, String>,
)
