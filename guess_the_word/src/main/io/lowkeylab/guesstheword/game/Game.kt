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
    private val playerLimit: UInt = 2u,
    private val players: MutableList<PlayerId> = mutableListOf(),
    private val rounds: MutableList<Round> = mutableListOf(),
    private var state: GameState = GameState.WAITING_FOR_PLAYERS,
) {
    private val currentRound: Round?
        get() = rounds.lastOrNull()

    fun addPlayer(playerId: PlayerId): Game {
        check(state == GameState.WAITING_FOR_PLAYERS) { "Cannot add player. Game is in $state state." }
        if (players.contains(playerId)) return this
        players.add(playerId)
        if (players.size == playerLimit.toInt()) {
            rounds.add(Round(number = 1u))
            state = GameState.IN_PROGRESS
        }
        return this
    }

    fun addGuess(
        playerId: PlayerId,
        guess: String,
    ): Game {
        check(state == GameState.IN_PROGRESS) { "Cannot add guess. Game is in $state state." }
        val round = checkNotNull(currentRound) { "Cannot add guess. No current round." }
        if (round.guesses.containsKey(playerId)) return this
        round.guesses[playerId] = guess

        if (round.guesses.size == players.size) {
            if (round.uniqueGuesses.size == 1) {
                state = GameState.COMPLETED
            } else {
                rounds.add(Round(number = round.number + 1u))
            }
        }
        return this
    }

    fun removePlayer(playerId: PlayerId): Game {
        check(state != GameState.COMPLETED) { "Cannot remove player. Game is in $state state." }
        players.remove(playerId)
        if (state == GameState.IN_PROGRESS) {
            state = GameState.COMPLETED
        }
        return this
    }

    fun hasEnded(): Boolean = state == GameState.COMPLETED

    fun isInProgress(): Boolean = state == GameState.IN_PROGRESS
}

@Serializable
data class Round(
    val number: UInt,
    val guesses: MutableMap<PlayerId, String> = mutableMapOf(),
) {
    val uniqueGuesses: Set<String>
        get() = guesses.values.toSet()
}

@Serializable
enum class GameState {
    WAITING_FOR_PLAYERS,
    IN_PROGRESS,
    COMPLETED,
}
