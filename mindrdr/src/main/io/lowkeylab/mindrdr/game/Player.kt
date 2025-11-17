package io.lowkeylab.mindrdr.game

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
