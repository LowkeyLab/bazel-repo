package io.lowkeylab.mindrdr

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.response.respond
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.lowkeylab.mindrdr.game.GameId
import io.lowkeylab.mindrdr.game.GameService
import kotlinx.serialization.Serializable

fun Application.configureGames(gameService: GameService) {
    routing {
        route("/games") {
            get {
                val games = gameService.getAllGames()
                call.respond(games)
            }
            post {
                val newGame = gameService.createGame()
                call.respond(newGame)
            }
            route("/{id}") {
                get {
                    val id = call.parameters["id"] ?: return@get call.respond(HttpStatusCode.BadRequest)
                    val game = gameService.getGameById(GameId(id)) ?: return@get call.respond(HttpStatusCode.NotFound)
                    call.respond(game)
                }
            }
        }
    }
}

@Serializable
data class AddGuessRequest(
    val guess: String,
)
