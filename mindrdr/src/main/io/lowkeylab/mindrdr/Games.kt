package io.lowkeylab.mindrdr

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.response.respond
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.ktor.server.websocket.webSocket
import io.ktor.websocket.CloseReason
import io.ktor.websocket.Frame
import io.ktor.websocket.WebSocketSession
import io.ktor.websocket.close
import io.ktor.websocket.readText
import io.lowkeylab.mindrdr.game.Game
import io.lowkeylab.mindrdr.game.GameId
import io.lowkeylab.mindrdr.game.GameService
import io.lowkeylab.mindrdr.game.Player
import kotlinx.coroutines.channels.consumeEach
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.util.concurrent.ConcurrentHashMap

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

                webSocket("/live") {
                    val gameIdParam =
                        call.parameters["id"] ?: run {
                            close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Missing game ID"))
                            return@webSocket
                        }
                    val gameId = GameId(gameIdParam)

                    // Validate game exists
                    gameService.getGameById(gameId) ?: run {
                        close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Game not found"))
                        return@webSocket
                    }

                    // Create and add player to game
                    val player =
                        try {
                            gameService.addPlayerToGame(gameId)
                        } catch (e: Exception) {
                            close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Failed to join game: ${e.message}"))
                            return@webSocket
                        }

                    // Track this session
                    val playerSession = PlayerSession(player, this)
                    gameSessions.computeIfAbsent(gameId) { ConcurrentHashMap.newKeySet() }.add(playerSession)

                    try {
                        // Notify player they joined
                        val joinMessage = OutgoingMessage.PlayerJoined(player)
                        send(Frame.Text(Json.encodeToString(OutgoingMessage.serializer(), joinMessage)))

                        // Broadcast current game state to all players
                        val updatedGame = gameService.getGameById(gameId)!!
                        broadcastToGame(gameId, OutgoingMessage.GameState(updatedGame))

                        // Handle incoming messages
                        incoming.consumeEach { frame ->
                            if (frame is Frame.Text) {
                                val text = frame.readText()
                                try {
                                    when (val message = Json.decodeFromString(IncomingMessage.serializer(), text)) {
                                        is IncomingMessage.SubmitGuess -> {
                                            try {
                                                gameService.addGuessToGame(player, gameId, message.guess)

                                                // Broadcast updated game state
                                                val currentGame = gameService.getGameById(gameId)!!
                                                broadcastToGame(gameId, OutgoingMessage.GameState(currentGame))

                                                // Check if game ended
                                                if (currentGame.hasEnded()) {
                                                    broadcastToGame(gameId, OutgoingMessage.GameTerminated("Game completed"))
                                                    closeAllGameSessions(gameId, "Game completed")
                                                }
                                            } catch (e: Exception) {
                                                send(
                                                    Frame.Text(
                                                        Json.encodeToString(
                                                            OutgoingMessage.serializer(),
                                                            OutgoingMessage.Error("Failed to submit guess: ${e.message}"),
                                                        ),
                                                    ),
                                                )
                                            }
                                        }
                                    }
                                } catch (e: Exception) {
                                    send(
                                        Frame.Text(
                                            Json.encodeToString(
                                                OutgoingMessage.serializer(),
                                                OutgoingMessage.Error("Invalid message format: ${e.message}"),
                                            ),
                                        ),
                                    )
                                }
                            }
                        }
                    } finally {
                        // Remove this session from tracking
                        gameSessions[gameId]?.remove(playerSession)

                        // Player disconnected - remove from game and terminate all connections
                        try {
                            gameService.removePlayerFromGame(player, gameId)

                            // Broadcast termination message and close all sessions
                            broadcastToGame(gameId, OutgoingMessage.GameTerminated("Player ${player.name.name} left the game"))
                            closeAllGameSessions(gameId, "Player left the game")
                        } catch (e: Exception) {
                            // Game might already be completed or player already removed
                        }
                    }
                }
            }
        }
    }
}

@Serializable
data class AddGuessRequest(
    val guess: String,
)

// WebSocket session tracking
private data class PlayerSession(
    val player: Player,
    val session: WebSocketSession,
)

private val gameSessions = ConcurrentHashMap<GameId, MutableSet<PlayerSession>>()

// WebSocket message types
@Serializable
sealed class IncomingMessage {
    @Serializable
    @SerialName("submit_guess")
    data class SubmitGuess(
        val guess: String,
    ) : IncomingMessage()
}

@Serializable
sealed class OutgoingMessage {
    @Serializable
    @SerialName("game_state")
    data class GameState(
        val game: Game,
    ) : OutgoingMessage()

    @Serializable
    @SerialName("game_terminated")
    data class GameTerminated(
        val reason: String,
    ) : OutgoingMessage()

    @Serializable
    @SerialName("error")
    data class Error(
        val message: String,
    ) : OutgoingMessage()

    @Serializable
    @SerialName("player_joined")
    data class PlayerJoined(
        val player: Player,
    ) : OutgoingMessage()
}

// Helper function to broadcast messages to all sessions in a game
private suspend fun broadcastToGame(
    gameId: GameId,
    message: OutgoingMessage,
) {
    val sessions = gameSessions[gameId] ?: return
    val json = Json.encodeToString(OutgoingMessage.serializer(), message)
    sessions.forEach { playerSession ->
        try {
            playerSession.session.send(Frame.Text(json))
        } catch (e: Exception) {
            // Session might be closed, ignore
        }
    }
}

// Helper function to close all sessions for a game
private suspend fun closeAllGameSessions(
    gameId: GameId,
    reason: String,
) {
    val sessions = gameSessions.remove(gameId) ?: return
    sessions.forEach { playerSession ->
        try {
            playerSession.session.close(CloseReason(CloseReason.Codes.NORMAL, reason))
        } catch (e: Exception) {
            // Session might already be closed, ignore
        }
    }
}
