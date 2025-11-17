package io.lowkeylab.mindrdr.game

class GameService(
    private val playerFactory: PlayerFactory,
    private val gameRepository: GameRepository,
) {
    suspend fun getAllGames(): List<Game> = gameRepository.getAllGames()

    suspend fun createGame(): Game = gameRepository.newGame()

    suspend fun getGameById(gameId: GameId): Game? = gameRepository.getGameById(gameId)

    suspend fun addPlayerToGame(gameId: GameId) {
        val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
        val player = playerFactory.create()
        game.addPlayer(player)
    }

    suspend fun removePlayerFromGame(
        player: Player,
        gameId: GameId,
    ) {
        val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
        game.removePlayer(player)
    }

    suspend fun addGuessToGame(
        player: Player,
        gameId: GameId,
        guess: String,
    ) {
        val game = requireNotNull(gameRepository.getGameById(gameId)) { "Game with id $gameId not found" }
        game.addGuess(player, guess)
    }
}
