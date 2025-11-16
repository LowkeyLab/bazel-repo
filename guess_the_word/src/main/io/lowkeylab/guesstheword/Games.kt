package io.lowkeylab.guesstheword

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.response.respond
import io.ktor.server.routing.delete
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.lowkeylab.guesstheword.game.GameService

fun Application.configureGames(gameService: GameService) {
    routing {
        route("/games") {
            get {
                val games = gameService.getAllGames()
                call.respond(games)
            }
            get("/{id}") {
                val id = call.parameters["id"] ?: return@get call.respond(HttpStatusCode.BadRequest)
                val game = gameService.getGameById(id) ?: return@get call.respond(HttpStatusCode.NotFound)
                call.respond(game)
            }
            post {
                val newGame = gameService.createGame()
                call.respond(newGame)
            }
            delete("/{id}") {
                val id = call.parameters["id"] ?: return@delete call.respond(HttpStatusCode.BadRequest)
                gameService.deleteGame(id)
                call.respond(HttpStatusCode.NoContent)
            }
        }
    }
}
