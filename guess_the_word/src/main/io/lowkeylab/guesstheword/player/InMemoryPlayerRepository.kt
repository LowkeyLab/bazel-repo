package io.lowkeylab.guesstheword.player

class InMemoryPlayerRepository : PlayerRepository {
    private val players = mutableMapOf<PlayerId, Player>()

    override suspend fun getAllPlayers(): List<Player> = players.values.toList()

    override suspend fun createPlayer(name: String): Player {
        val newPlayerId = PlayerId("${players.size + 1}")
        val newPlayer =
            Player(
                id = newPlayerId,
                name = name,
            )
        players[newPlayerId] = newPlayer
        return newPlayer
    }

    override suspend fun getPlayerById(playerId: PlayerId): Player? = players[playerId]

    override suspend fun deletePlayer(playerId: PlayerId) {
        players.remove(playerId)
    }
}
