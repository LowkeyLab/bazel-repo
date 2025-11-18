package io.lowkeylab.mindrdr.game

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

class GameService(
    private val playerFactory: PlayerFactory,
    private val gameRepository: GameRepository,
) {
    private val mutex = Mutex()

    suspend fun getAllGames(): List<Game> = gameRepository.getAllGames()

    suspend fun createGame(): Game = gameRepository.newGame()

    suspend fun getGameById(gameId: GameId): Game? = gameRepository.getGameById(gameId)

    suspend fun addPlayerToGame(gameId: GameId): Player {
        mutex.withLock {
            val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
            val player = playerFactory.create()
            game.addPlayer(player)
            return player
        }
    }

    suspend fun removePlayerFromGame(
        player: Player,
        gameId: GameId,
    ) {
        mutex.withLock {
            val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
            game.removePlayer(player)
        }
    }

    suspend fun addGuessToGame(
        player: Player,
        gameId: GameId,
        guess: String,
    ) {
        mutex.withLock {
            val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
            game.addGuess(player, guess)
        }
    }
}
