package io.lowkeylab.mindreadr.game

interface PlayerFactory {
  fun create(): Player

  fun removeName(name: PlayerName)
}
