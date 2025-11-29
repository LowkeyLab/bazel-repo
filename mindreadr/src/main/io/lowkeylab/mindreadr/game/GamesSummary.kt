package io.lowkeylab.mindreadr.game

import kotlinx.serialization.Serializable

@Serializable
data class GamesSummary(
    val waitingForPlayerGames: Int,
    val inProgressGames: Int,
    val completedGames: Int,
)
