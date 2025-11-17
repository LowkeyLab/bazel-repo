package io.lowkeylab.mindrdr.player

import kotlinx.serialization.Serializable

@JvmInline
@Serializable
value class PlayerId(
    val id: String,
)

@Serializable
class Player(
    val id: PlayerId,
    val name: String,
)
