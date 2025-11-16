package io.lowkeylab.guesstheword.game

class GameService(
    private val gameRepository: GameRepository,
) {
    suspend fun getAllGames(): List<Game> = gameRepository.getAllGames()

    suspend fun createGame(): Game = gameRepository.newGame()

    suspend fun getGameById(gameId: String): Game? = gameRepository.getGameById(gameId)

    suspend fun deleteGame(gameId: String) = gameRepository.deleteGame(gameId)
}
