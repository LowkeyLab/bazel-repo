package io.lowkeylab.mindreadr

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.application.log
import io.ktor.server.response.respond
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import io.ktor.server.websocket.sendSerialized
import io.ktor.server.websocket.webSocket
import io.ktor.websocket.CloseReason
import io.ktor.websocket.Frame
import io.ktor.websocket.close
import io.ktor.websocket.readText
import io.lowkeylab.mindreadr.game.Game
import io.lowkeylab.mindreadr.game.GameId
import io.lowkeylab.mindreadr.game.GameService
import io.lowkeylab.mindreadr.game.GameState
import io.lowkeylab.mindreadr.game.Player
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.util.concurrent.ConcurrentHashMap
import kotlin.collections.component1
import kotlin.collections.component2

@Serializable
data class RoundDto(
    val number: UInt,
    val guesses: Map<String, String>, // player name -> guess
)

@Serializable
data class GameDto(
    val id: GameId,
    val playerLimit: UInt,
    val players: List<Player>,
    val rounds: List<RoundDto>,
    val state: GameState,
    val finalGuess: String? = null,
)

@Serializable
data class GamesSummary(
    val waitingForPlayerGames: Int,
    val inProgressGames: Int,
    val completedGames: Int,
)

fun Game.toDto() =
    GameDto(
        id = id,
        playerLimit = getPlayerLimit(),
        players = getPlayers(),
        rounds =
            getRounds().map { r ->
                RoundDto(
                    number = r.number,
                    guesses = r.guesses.map { (player, guess) -> player.name.name to guess }.toMap(),
                )
            },
        state = getState(),
        finalGuess = getFinalGuess(),
    )

fun Application.configureGames(gameService: GameService) {
    routing {
        route("/games") {
            get {
                val games = gameService.getAllGames().map { it.toDto() }
                call.respond(games)
            }
            get("/summary") {
                val inProgress = gameService.countGamesByState(GameState.IN_PROGRESS)
                val completed = gameService.countGamesByState(GameState.COMPLETED)
                val waiting = gameService.countGamesByState(GameState.WAITING_FOR_PLAYERS)
                val summary =
                    GamesSummary(
                        waitingForPlayerGames = waiting.toInt(),
                        inProgressGames = inProgress.toInt(),
                        completedGames = completed.toInt(),
                    )
                call.respond(summary)
            }
            post {
                val newGame = gameService.createGame().toDto()
                call.respond(newGame)
            }
            route("/{id}") {
                get {
                    val id = call.parameters["id"] ?: return@get call.respond(HttpStatusCode.BadRequest)
                    val game = gameService.getGameById(GameId(id)) ?: return@get call.respond(HttpStatusCode.NotFound)
                    call.respond(game.toDto())
                }

                webSocket("/live") {
                    val gameIdParam =
                        call.parameters["id"] ?: run {
                            this@webSocket.close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Missing game ID"))
                            return@webSocket
                        }
                    val gameId = GameId(gameIdParam)

                    // Validate game exists
                    gameService.getGameById(gameId) ?: run {
                        this@webSocket.close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Game not found"))
                        return@webSocket
                    }

                    // Create and add player to game
                    val player =
                        try {
                            gameService.addPlayerToGame(gameId)
                        } catch (e: Exception) {
                            log.error("Failed to add player to game {}. {}", gameId.id, e.message)
                            this@webSocket.close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Failed to join game: ${e.message}"))
                            return@webSocket
                        }

                    // Create or get the shared flow for this game
                    val sharedFlow =
                        gameFlows.computeIfAbsent(gameId) {
                            MutableSharedFlow()
                        }

                    // Launch a coroutine to forward flow emissions to this client
                    val job =
                        launch {
                            sharedFlow.asSharedFlow().collect { message ->
                                try {
                                    when (message) {
                                        is OutgoingMessage.GameTerminated -> {
                                            sendSerialized<OutgoingMessage>(message)
                                            log.info(
                                                "Closing WebSocket for player {} in game {}. Reason is {}.",
                                                player.name.name,
                                                gameId.id,
                                                message.reason,
                                            )
                                            this@webSocket.close(CloseReason(CloseReason.Codes.NORMAL, message.reason))
                                        }
                                        else -> sendSerialized<OutgoingMessage>(message)
                                    }
                                } catch (_: Exception) {
                                    // Ignore send/close failures (client may be gone)
                                }
                            }
                        }

                    try {
                        // Notify player they joined (only to this client)
                        log.info("Player {} joined game {}", player.name.name, gameId.id)
                        sendSerialized<OutgoingMessage>(OutgoingMessage.PlayerJoined(player))

                        // Broadcast current game state to all players via the shared flow
                        val updatedGame = gameService.getGameById(gameId)!!.toDto()
                        sharedFlow.emit(OutgoingMessage.GameState(updatedGame))

                        // Handle incoming messages
                        for (frame in incoming) {
                            if (frame is Frame.Text) {
                                runCatching {
                                    when (val message = Json.decodeFromString(IncomingMessage.serializer(), frame.readText())) {
                                        is IncomingMessage.SubmitGuess -> {
                                            runCatching {
                                                log.info(
                                                    "Player {} submitting guess in game {}: {}",
                                                    player.name.name,
                                                    gameId.id,
                                                    message.guess,
                                                )

                                                gameService.addGuessToGame(player, gameId, message.guess)

                                                // Broadcast updated game state
                                                val currentGame = gameService.getGameById(gameId)!!.toDto()
                                                sharedFlow.emit(OutgoingMessage.GameState(currentGame))

                                                // Check if game ended
                                                if (currentGame.state == GameState.COMPLETED) {
                                                    // Emit termination to all clients
                                                    sharedFlow.emit(OutgoingMessage.GameTerminated("Game completed"))
                                                }
                                            }.onFailure { e ->
                                                log.error(
                                                    "Failed to submit guess for player ${player.name.name} in game $gameId: ${e.message}",
                                                )
                                                // Per-client error message
                                                sendSerialized<OutgoingMessage>(
                                                    OutgoingMessage.Error("Failed to submit guess: ${e.message}"),
                                                )
                                            }
                                        }
                                    }
                                }.onFailure { e ->
                                    // Per-client error message for bad input
                                    sendSerialized<OutgoingMessage>(OutgoingMessage.Error("Invalid message format: ${e.message}"))
                                }
                            }
                        }
                    } finally {
                        // Player disconnected - remove from game and terminate all connections
                        runCatching { gameService.removePlayerFromGame(player, gameId) }

                        // Emit termination to all clients (best-effort, non-suspending if no subscribers)
                        gameFlows[gameId]?.emit(OutgoingMessage.GameTerminated("Player ${player.name.name} left the game"))

                        // Cancel the collection job before cleanup
                        job.cancel()

                        // Remove the flow for this game (allow recreation on next connect)
                        gameFlows.remove(gameId)
                    }
                }
            }
        }
    }
}

// Replace session tracking with per-game shared flows
private val gameFlows = ConcurrentHashMap<GameId, MutableSharedFlow<OutgoingMessage>>()

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
        val game: GameDto,
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
