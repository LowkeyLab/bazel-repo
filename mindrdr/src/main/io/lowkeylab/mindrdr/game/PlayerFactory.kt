package io.lowkeylab.mindrdr.game

interface PlayerFactory {
    fun create(): Player

    fun removeName(name: PlayerName)
}
