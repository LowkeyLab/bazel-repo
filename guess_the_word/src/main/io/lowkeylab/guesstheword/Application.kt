package io.lowkeylab.guesstheword

import io.ktor.server.application.Application
import io.ktor.server.engine.embeddedServer
import io.ktor.server.netty.Netty
import io.lowkeylab.guesstheword.game.GameService
import io.lowkeylab.guesstheword.game.InMemoryGameRepository
import io.lowkeylab.guesstheword.player.InMemoryPlayerRepository
import io.lowkeylab.guesstheword.player.PlayerServiceFactory

fun main() {
    embeddedServer(Netty, port = 8080, host = "0.0.0.0", module = Application::module)
        .start(wait = true)
}

fun Application.module() {
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(gameRepository)

    val playerRepository = InMemoryPlayerRepository()
    val playerService = PlayerServiceFactory(playerRepository).fromClasspath()

    configureSockets()
    configureSerialization()
    configureRouting()
    configureGames(gameService)
    configurePlayers(playerService)
}
