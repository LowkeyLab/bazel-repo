package io.lowkeylab.mindreadr.game

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

    suspend fun countGamesByState(state: GameState): Long = gameRepository.countGamesByState(state)

    suspend fun getGamesByState(state: GameState): List<Game> = gameRepository.getGamesByState(state)

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
    ): Game {
        mutex.withLock {
            val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
            game.removePlayer(player)
            return game
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

    suspend fun removeGame(gameId: GameId) {
        mutex.withLock {
            gameRepository.deleteGame(gameId)
        }
    }
}
