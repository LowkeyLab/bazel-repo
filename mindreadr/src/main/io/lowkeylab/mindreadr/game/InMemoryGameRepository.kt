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

    override suspend fun countGamesByState(state: GameState): Long =
        games.values
            .count {
                when (state) {
                    GameState.WAITING_FOR_PLAYERS -> it.getState() == GameState.WAITING_FOR_PLAYERS
                    GameState.IN_PROGRESS -> it.getState() == GameState.IN_PROGRESS
                    GameState.COMPLETED -> it.getState() == GameState.COMPLETED
                    GameState.TERMINATED -> it.getState() == GameState.TERMINATED
                }
            }.toLong()

    override suspend fun getGamesByState(state: GameState): List<Game> =
        games.values.filter {
            when (state) {
                GameState.WAITING_FOR_PLAYERS -> it.getState() == GameState.WAITING_FOR_PLAYERS
                GameState.IN_PROGRESS -> it.getState() == GameState.IN_PROGRESS
                GameState.COMPLETED -> it.getState() == GameState.COMPLETED
                GameState.TERMINATED -> it.getState() == GameState.TERMINATED
            }
        }

    override suspend fun deleteGame(gameId: GameId) {
        games.remove(gameId)
    }
}
