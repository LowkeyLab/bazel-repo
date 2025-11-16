package io.lowkeylab.guesstheword.game

interface GameRepository {
    suspend fun getAllGames(): List<Game>

    suspend fun newGame(): Game

    suspend fun getGameById(gameId: String): Game?

    suspend fun deleteGame(gameId: String)
}
