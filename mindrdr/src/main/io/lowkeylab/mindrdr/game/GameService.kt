package io.lowkeylab.mindrdr.game

import io.lowkeylab.mindrdr.player.PlayerId

class GameService(
    private val gameRepository: GameRepository,
) {
    suspend fun getAllGames(): List<Game> = gameRepository.getAllGames()

    suspend fun createGame(): Game = gameRepository.newGame()

    suspend fun getGameById(gameId: GameId): Game? = gameRepository.getGameById(gameId)

    suspend fun addPlayerToGame(
        playerId: PlayerId,
        gameId: GameId,
    ) {
        val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
        game.addPlayer(playerId)
    }

    suspend fun removePlayerFromGame(
        playerId: PlayerId,
        gameId: GameId,
    ) {
        val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
        game.removePlayer(playerId)
    }

    suspend fun addGuessToGame(
        playerId: PlayerId,
        gameId: GameId,
        guess: String,
    ) {
        val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
        game.addGuess(playerId, guess)
    }
}
