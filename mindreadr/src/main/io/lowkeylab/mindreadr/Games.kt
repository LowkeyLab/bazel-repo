package io.lowkeylab.mindreadr

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.application.log
import io.ktor.server.response.respond
import io.ktor.server.response.respondText
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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.util.concurrent.ConcurrentHashMap

@Serializable
data class RoundDto(
    val number: UInt,
    val guesses: Map<String, String>, // player name -> guess
)

@Serializable
data class GameDto(
    val id: GameId,
    val playerLimit: UInt,
    val roundLimit: UInt,
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
        roundLimit = getRoundLimit(),
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

fun Application.configureGamesWs(gameService: GameService) {
    routing {
        webSocket("/games/{id}/live") {
            val gameIdParam =
                call.parameters["id"] ?: run {
                    this@webSocket.close(CloseReason(CloseReason.Codes.VIOLATED_POLICY, "Missing game ID"))
                    return@webSocket
                }
            log.info("WebSocket connection requested for game ID: $gameIdParam")
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
                    this@webSocket.close(
                        CloseReason(
                            CloseReason.Codes.VIOLATED_POLICY,
                            "Failed to join game: ${e.message}",
                        ),
                    )
                    return@webSocket
                }

            // Create or get the shared flow for this game
            val broadcastFlow =
                broadcastFlows.computeIfAbsent(gameId) {
                    MutableSharedFlow()
                }

            val terminationFlow =
                terminationFlows.computeIfAbsent(gameId) {
                    MutableSharedFlow()
                }

            val broadcastJob =
                launch {
                    broadcastFlow.asSharedFlow().collect { message ->
                        sendSerialized<OutgoingMessage>(message)

                        if (message is OutgoingMessage.GameState) {
                            if (message.game.state == GameState.COMPLETED || message.game.state == GameState.TERMINATED) {
                                log.info(
                                    "Game {} has completed. Emitting termination signal.",
                                    gameId.id,
                                )
                                terminationFlow.emit(Unit)
                            }
                        }
                    }
                }

            launch {
                // Wait for termination signal
                terminationFlow.asSharedFlow().first()

                log.info(
                    "Terminating WebSocket for player {} in game {} due to game termination flow.",
                    player.name.name,
                    gameId.id,
                )

                broadcastFlows.remove(gameId)
                terminationFlows.remove(gameId)

                broadcastJob.cancel()

                this@webSocket.close(CloseReason(CloseReason.Codes.NORMAL, "Game has ended"))
            }

            try {
                log.info("Player {} joined game {}", player.name.name, gameId.id)
                sendSerialized<OutgoingMessage>(OutgoingMessage.PlayerJoined(player))

                // Broadcast current game state to all players via the shared flow
                val updatedGame = gameService.getGameById(gameId)!!.toDto()
                broadcastFlow.emit(OutgoingMessage.GameState(updatedGame))

                // Handle incoming messages
                for (frame in incoming) {
                    if (frame is Frame.Text) {
                        runCatching {
                            when (
                                val message =
                                    Json.decodeFromString(IncomingMessage.serializer(), frame.readText())
                            ) {
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
                                        broadcastFlow.emit(OutgoingMessage.GameState(currentGame))
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
                log.info("Player {} disconnected from game {}", player.name.name, gameId.id)

                val game = gameService.getGameById(gameId)
                if (game != null) {
                    if (game.hasEnded()) {
                        log.info("Game $gameId has already ended. Closing connections.")
                        terminationFlow.emit(Unit)
                    } else {
                        val game = gameService.removePlayerFromGame(player, gameId)
                        log.info("Player ${player.name.name} removed from game $gameId")
                        // Notify remaining players
                        broadcastFlow.emit(OutgoingMessage.PlayerLeft(player))
                        broadcastFlow.emit(OutgoingMessage.GameState(game.toDto()))
                    }
                } else {
                    log.info("Game $gameId not found during disconnect of player ${player.name.name}. Closing connections.")
                    terminationFlow.emit(Unit)
                }
            }
        }
    }
}

fun Application.configureGames(gameService: GameService) {
    routing {
        route("/games") {
            get {
                val status = call.request.queryParameters["status"]
                if (status != null) {
                    val state =
                        when (status.lowercase()) {
                            "waiting_for_players" -> GameState.WAITING_FOR_PLAYERS
                            "in_progress" -> GameState.IN_PROGRESS
                            "completed" -> GameState.COMPLETED
                            else -> return@get call.respond(HttpStatusCode.BadRequest, "Invalid status parameter")
                        }
                    val gamesByState = gameService.getGamesByState(state).map { it.toDto() }
                    return@get call.respond(gamesByState)
                }

                call.respondText("Please specify a status query parameter to filter games by state.")
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
            }
        }
    }
}

private val broadcastFlows = ConcurrentHashMap<GameId, MutableSharedFlow<OutgoingMessage>>()

private val terminationFlows = ConcurrentHashMap<GameId, MutableSharedFlow<Unit>>()

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
    @SerialName("error")
    data class Error(
        val message: String,
    ) : OutgoingMessage()

    @Serializable
    @SerialName("player_joined")
    data class PlayerJoined(
        val player: Player,
    ) : OutgoingMessage()

    @Serializable
    @SerialName("player_left")
    data class PlayerLeft(
        val player: Player,
    ) : OutgoingMessage()
}
