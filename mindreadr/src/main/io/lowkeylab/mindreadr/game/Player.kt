package io.lowkeylab.mindreadr.game

import kotlinx.serialization.Serializable

@JvmInline
@Serializable
value class PlayerName(
    val name: String,
)

@Serializable
data class Player(
    val name: PlayerName,
)
