package io.lowkeylab.mindrdr.player

interface PlayerRepository {
    suspend fun getAllPlayers(): List<Player>

    suspend fun createPlayer(name: String): Player

    suspend fun getPlayerById(playerId: PlayerId): Player?

    suspend fun deletePlayer(playerId: PlayerId)
}
