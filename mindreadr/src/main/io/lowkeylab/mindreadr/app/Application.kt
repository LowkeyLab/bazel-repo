package io.lowkeylab.mindreadr.app

import io.ktor.server.application.Application
import io.ktor.server.netty.EngineMain
import io.lowkeylab.mindreadr.configureGames
import io.lowkeylab.mindreadr.configureObservability
import io.lowkeylab.mindreadr.configureRouting
import io.lowkeylab.mindreadr.configureSecurity
import io.lowkeylab.mindreadr.configureSerialization
import io.lowkeylab.mindreadr.configureSockets
import io.lowkeylab.mindreadr.game.GameService
import io.lowkeylab.mindreadr.game.InMemoryGameRepository
import io.lowkeylab.mindreadr.game.ResourcePlayerFactory

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
    configureObservability()
    configureGames(gameService)
}
