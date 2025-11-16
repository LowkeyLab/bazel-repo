package io.lowkeylab.guesstheword

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.response.respond
import io.ktor.server.routing.delete
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.lowkeylab.guesstheword.player.PlayerId
import io.lowkeylab.guesstheword.player.PlayerService

fun Application.configurePlayers(playerService: PlayerService) {
    routing {
        route("/players") {
            get {
                playerService.getAllPlayers()
            }
            get("/{id}") {
                val id = call.parameters["id"] ?: return@get
                playerService.getPlayerById(PlayerId(id)) ?: return@get call.respond(HttpStatusCode.NotFound, "Player not found")
            }
            post {
                val newPlayer = playerService.createPlayer()
                call.respond(newPlayer)
            }
            delete("/{id}") {
                val id = call.parameters["id"] ?: return@delete call.respond(HttpStatusCode.BadRequest)
                playerService.deletePlayer(PlayerId(id))
                call.respond(HttpStatusCode.NoContent)
            }
        }
    }
}
