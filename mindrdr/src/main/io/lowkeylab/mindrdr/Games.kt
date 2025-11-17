package io.lowkeylab.mindrdr

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.request.receive
import io.ktor.server.response.respond
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.ktor.server.sessions.get
import io.ktor.server.sessions.sessions
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
                post("/join") {
                    val id = call.parameters["id"] ?: return@post call.respond(HttpStatusCode.BadRequest)
                    val playerId = call.sessions.get<PlayerSession>()?.id ?: return@post call.respond(HttpStatusCode.Unauthorized)
                    gameService.addPlayerToGame(playerId, GameId(id))
                    call.respond(HttpStatusCode.OK, "Player added to game")
                }
                post("/leave") {
                    val id = call.parameters["id"] ?: return@post call.respond(HttpStatusCode.BadRequest)
                    val playerId = call.sessions.get<PlayerSession>()?.id ?: return@post call.respond(HttpStatusCode.Unauthorized)
                    gameService.removePlayerFromGame(playerId, GameId(id))
                    call.respond(HttpStatusCode.OK, "Player removed from game")
                }
                post("/addGuess") {
                    val guess = call.receive<AddGuessRequest>()
                    val id = call.parameters["id"] ?: return@post call.respond(HttpStatusCode.BadRequest)
                    val playerId = call.sessions.get<PlayerSession>()?.id ?: return@post call.respond(HttpStatusCode.Unauthorized)
                    gameService.addGuessToGame(playerId, GameId(id), guess.guess)
                    call.respond(HttpStatusCode.OK, "Guess added to game")
                }
            }
        }
    }
}

@Serializable
data class AddGuessRequest(
    val guess: String,
)
