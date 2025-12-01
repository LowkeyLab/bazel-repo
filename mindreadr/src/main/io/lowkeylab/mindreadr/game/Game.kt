package io.lowkeylab.mindreadr.game

import kotlinx.serialization.Serializable
import java.util.Locale

@JvmInline
@Serializable
value class GameId(
    val id: String,
)

class Game(
    val id: GameId,
    private val playerLimit: UInt = 2u,
    private val turnLimit: UInt = 10u,
    private val players: MutableList<Player> = mutableListOf(),
    private val rounds: MutableList<Round> = mutableListOf(),
    private var state: GameState = GameState.WAITING_FOR_PLAYERS,
) {
    private val currentRound: Round?
        get() = rounds.lastOrNull()

    fun addPlayer(player: Player): Game {
        check(state == GameState.WAITING_FOR_PLAYERS) { "Cannot add player. Game is in $state state." }
        players.add(player)
        if (players.size == playerLimit.toInt()) {
            rounds.add(Round(number = 1u))
            state = GameState.IN_PROGRESS
        }
        return this
    }

    fun addGuess(
        player: Player,
        guess: String,
    ): Game {
        check(state == GameState.IN_PROGRESS) { "Cannot add guess. Game is in $state state." }
        val round = checkNotNull(currentRound) { "Cannot add guess. No current round." }
        if (round.guesses.containsKey(player)) return this
        // Trim and reject empty / whitespace-only guesses
        val trimmed = guess.trim()
        check(trimmed.isNotEmpty()) { "Guess cannot be empty or whitespace." }
        // Normalize guess to lowercase (locale-independent)
        val normalized = trimmed.lowercase(Locale.ROOT)
        // Build case-insensitive set of previously used guesses
        val previouslyUsedGuesses =
            rounds
                .dropLast(1)
                .flatMap { it.guesses.values }
                .map { it.lowercase(Locale.ROOT) }
                .toSet()
        check(normalized !in previouslyUsedGuesses) { "Guess '$guess' has already been used (case-insensitive) in a previous round." }
        round.guesses[player] = normalized

        if (round.guesses.size == players.size) {
            if (round.uniqueGuesses.size == 1) {
                state = GameState.COMPLETED
            } else if (round.number >= turnLimit) {
                state = GameState.TERMINATED
            } else {
                rounds.add(Round(number = round.number + 1u))
            }
        }
        return this
    }

    fun removePlayer(player: Player): Game {
        check(state != GameState.COMPLETED) { "Cannot remove player. Game is in $state state." }
        players.remove(player)
        if (state == GameState.IN_PROGRESS) {
            state = GameState.TERMINATED
        }
        return this
    }

    fun hasEnded(): Boolean = state == GameState.COMPLETED || state == GameState.TERMINATED

    fun isInProgress(): Boolean = state == GameState.IN_PROGRESS

    fun isWaitingForPlayers(): Boolean = state == GameState.WAITING_FOR_PLAYERS

    fun getPlayerCount(): Int = players.size

    fun getRoundCount(): Int = rounds.size

    fun getFinalGuess(): String? =
        if (state == GameState.COMPLETED) {
            val currentRound = checkNotNull(currentRound) { "Game is completed but no current round." }
            currentRound.uniqueGuesses.first()
        } else {
            null
        }

    fun getPlayerLimit(): UInt = playerLimit

    fun getTurnLimit(): UInt = turnLimit

    fun getPlayers(): List<Player> = players.toList()

    fun getRounds(): List<Round> = rounds.toList()

    fun getState(): GameState = state
}

@Serializable
data class Round(
    val number: UInt,
    val guesses: MutableMap<Player, String> = mutableMapOf(),
) {
    val uniqueGuesses: Set<String>
        get() = guesses.values.toSet()
}

@Serializable
enum class GameState {
    WAITING_FOR_PLAYERS,
    IN_PROGRESS,
    COMPLETED,
    TERMINATED,
}
