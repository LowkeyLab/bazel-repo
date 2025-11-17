package io.lowkeylab.guesstheword

import io.ktor.server.application.Application
import io.lowkeylab.guesstheword.game.GameService
import io.lowkeylab.guesstheword.game.InMemoryGameRepository
import io.lowkeylab.guesstheword.player.InMemoryPlayerRepository
import io.lowkeylab.guesstheword.player.PlayerServiceFactory

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
