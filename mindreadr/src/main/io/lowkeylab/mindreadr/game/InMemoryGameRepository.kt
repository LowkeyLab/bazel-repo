package io.lowkeylab.mindreadr.game

class InMemoryGameRepository : GameRepository {
    private val games: MutableMap<GameId, Game> = mutableMapOf()

    override suspend fun getAllGames(): List<Game> = games.values.toList()

    override suspend fun newGame(): Game {
        val game = Game(id = GameId("${games.size + 1}"))
        games[game.id] = game
        return game
    }

    override suspend fun getGameById(gameId: GameId): Game? = games[gameId]

    override suspend fun deleteGame(gameId: GameId) {
        games.remove(gameId)
    }
}
