package io.lowkeylab.mindrdr.app

import io.ktor.server.application.Application
import io.ktor.server.netty.EngineMain
import io.lowkeylab.mindrdr.configureGames
import io.lowkeylab.mindrdr.configureRouting
import io.lowkeylab.mindrdr.configureSecurity
import io.lowkeylab.mindrdr.configureSerialization
import io.lowkeylab.mindrdr.configureSockets
import io.lowkeylab.mindrdr.game.GameService
import io.lowkeylab.mindrdr.game.InMemoryGameRepository
import io.lowkeylab.mindrdr.game.ResourcePlayerFactory

fun main(args: Array<String>) {
    EngineMain.main(args)
}

fun Application.module() {
    val playerFactory = ResourcePlayerFactory("adjectives.txt", "nouns.txt")
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    configureSecurity()
    configureSockets()
    configureSerialization()
    configureRouting()
    configureGames(gameService)
}
