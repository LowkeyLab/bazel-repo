package io.lowkeylab.mindrdr.app

import io.ktor.server.application.Application
import io.ktor.server.netty.EngineMain
import io.lowkeylab.mindrdr.configureGames
import io.lowkeylab.mindrdr.configurePlayers
import io.lowkeylab.mindrdr.configureRouting
import io.lowkeylab.mindrdr.configureSecurity
import io.lowkeylab.mindrdr.configureSerialization
import io.lowkeylab.mindrdr.configureSockets
import io.lowkeylab.mindrdr.game.GameService
import io.lowkeylab.mindrdr.game.InMemoryGameRepository
import io.lowkeylab.mindrdr.player.InMemoryPlayerRepository
import io.lowkeylab.mindrdr.player.PlayerServiceFactory

fun main(args: Array<String>) {
    EngineMain.main(args)
}

fun Application.module() {
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(gameRepository)

    val playerRepository = InMemoryPlayerRepository()
    val playerService = PlayerServiceFactory(playerRepository).fromClasspath()

    configureSecurity()
    configureSockets()
    configureSerialization()
    configureRouting()
    configureGames(gameService)
    configurePlayers(playerService)
}
