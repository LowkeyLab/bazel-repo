package io.lowkeylab.guesstheword

import io.ktor.server.application.Application
import io.ktor.server.response.respond
import io.ktor.server.routing.get
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.ktor.server.sessions.get
import io.ktor.server.sessions.sessions
import io.ktor.server.sessions.set
import io.lowkeylab.guesstheword.player.PlayerService

fun Application.configurePlayers(playerService: PlayerService) {
    routing {
        route("/players") {
            get("/login") {
                val playerSession = call.sessions.get<PlayerSession>()
                val player =
                    if (playerSession != null) {
                        playerService.getPlayerById(playerSession.id) ?: playerService.createPlayer()
                    } else {
                        playerService.createPlayer()
                    }
                call.sessions.set(PlayerSession(player.id, player.name))
                call.respond(player)
            }
        }
    }
}
