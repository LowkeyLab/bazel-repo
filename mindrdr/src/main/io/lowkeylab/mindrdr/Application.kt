package io.lowkeylab.mindrdr

import io.ktor.server.application.Application
import io.lowkeylab.mindrdr.game.GameService
import io.lowkeylab.mindrdr.game.InMemoryGameRepository
import io.lowkeylab.mindrdr.player.InMemoryPlayerRepository
import io.lowkeylab.mindrdr.player.PlayerServiceFactory

fun main(args: Array<String>) {
    io.ktor.server.netty.EngineMain
        .main(args)
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
