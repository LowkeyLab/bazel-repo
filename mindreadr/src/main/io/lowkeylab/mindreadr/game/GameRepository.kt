package io.lowkeylab.mindreadr.game

interface GameRepository {
    suspend fun getAllGames(): List<Game>

    suspend fun newGame(): Game

    suspend fun getGameById(gameId: GameId): Game?

    suspend fun deleteGame(gameId: GameId)
}
